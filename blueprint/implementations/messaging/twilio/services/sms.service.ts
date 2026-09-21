import { IdDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';
import { SmsMappers } from '@forklaunch/implementation-messaging-base/types';
import { SmsService } from '@forklaunch/interfaces-messaging/interfaces';
import { SendSmsDto } from '@forklaunch/interfaces-messaging/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { TwilioMessageStatusEnum } from '../domain/enum/twilioMessageStatus.enum';
import {
  TwilioMessageResource,
  TwilioSmsDtos,
  TwilioSmsEntities,
  TwilioStatusCallbackDto
} from '../domain/types/twilio.types';
import { SmsRecord } from '../persistence/entities';

/**
 * Maps a Twilio message status (from the Messages REST API or a delivery
 * status callback) onto the canonical SmsStatusEnum.
 */
export const mapTwilioMessageStatus = (
  messageStatus: string
): SmsStatusEnum => {
  switch (messageStatus) {
    case TwilioMessageStatusEnum.SENT:
      return SmsStatusEnum.SENT;
    case TwilioMessageStatusEnum.DELIVERED:
    case TwilioMessageStatusEnum.READ:
      return SmsStatusEnum.DELIVERED;
    case TwilioMessageStatusEnum.UNDELIVERED:
      return SmsStatusEnum.UNDELIVERED;
    case TwilioMessageStatusEnum.FAILED:
    case TwilioMessageStatusEnum.CANCELED:
      return SmsStatusEnum.FAILED;
    case TwilioMessageStatusEnum.QUEUED:
    case TwilioMessageStatusEnum.ACCEPTED:
    case TwilioMessageStatusEnum.SCHEDULED:
    case TwilioMessageStatusEnum.SENDING:
    default:
      return SmsStatusEnum.QUEUED;
  }
};

const TWILIO_API_BASE_URL = 'https://api.twilio.com/2010-04-01';

export class TwilioSmsService<
  SchemaValidator extends AnySchemaValidator,
  Entities extends TwilioSmsEntities = TwilioSmsEntities,
  Dto extends TwilioSmsDtos = TwilioSmsDtos
> implements SmsService<typeof SmsStatusEnum> {
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  protected readonly accountSid: string;
  protected readonly authToken: string;
  protected readonly fromNumber: string;
  public em: EntityManager;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: SmsMappers<typeof SmsStatusEnum, Entities, Dto>;

  constructor(
    accountSid: string,
    authToken: string,
    fromNumber: string,
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: SmsMappers<typeof SmsStatusEnum, Entities, Dto>,
    readonly options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
    this.accountSid = accountSid;
    this.authToken = authToken;
    this.fromNumber = fromNumber;
    this.em = em;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.evaluatedTelemetryOptions = options?.telemetry
      ? evaluateTelemetryOptions(options.telemetry).enabled
      : {
          logging: false,
          metrics: false,
          tracing: false
        };
  }

  async sendSms(
    smsDto: SendSmsDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<Dto['SmsRecordMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Sending sms via twilio', {
        to: smsDto.to
      });
    }

    const smsRecord = await this.mappers.SendSmsMapper.toEntity(
      smsDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    const record = smsRecord as {
      status: SmsStatusEnum;
      providerMessageId?: string | null;
      error?: string | null;
    };

    // the twilio package is intentionally dependency-free — the message is
    // dispatched via the Twilio Messages REST API with plain fetch
    const response = await fetch(
      `${TWILIO_API_BASE_URL}/Accounts/${this.accountSid}/Messages.json`,
      {
        method: 'POST',
        headers: {
          Authorization: `Basic ${Buffer.from(
            `${this.accountSid}:${this.authToken}`
          ).toString('base64')}`,
          'Content-Type': 'application/x-www-form-urlencoded'
        },
        body: new URLSearchParams({
          To: smsDto.to,
          From: this.fromNumber,
          Body: smsDto.body
        }).toString()
      }
    );

    if (response.ok) {
      const message = (await response.json()) as TwilioMessageResource;
      record.providerMessageId = message.sid;
      record.status = mapTwilioMessageStatus(message.status);
      record.error = message.error_message ?? null;
    } else {
      const errorBody = await response.text();
      record.status = SmsStatusEnum.FAILED;
      record.error = `Twilio API error (${response.status}): ${errorBody}`;
      this.openTelemetryCollector.error('Twilio message dispatch failed', {
        status: response.status,
        error: errorBody
      });
    }

    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(smsRecord);
    });

    return this.mappers.SmsRecordMapper.toDto(
      smsRecord as InferEntity<Entities['SmsRecordMapper']>
    );
  }

  async getSmsRecord(
    { id }: IdDto,
    em?: EntityManager
  ): Promise<Dto['SmsRecordMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting sms record', { id });
    }

    const smsRecord = await (em ?? this.em).findOneOrFail(
      this.mappers.SmsRecordMapper.entity as typeof SmsRecord,
      { id }
    );

    return this.mappers.SmsRecordMapper.toDto(
      smsRecord as InferEntity<Entities['SmsRecordMapper']>
    );
  }

  /**
   * Maps a Twilio delivery-status callback (MessageStatus, MessageSid,
   * ErrorMessage) onto the persisted record's status.
   */
  async processStatusCallback(
    payload: TwilioStatusCallbackDto,
    em?: EntityManager
  ): Promise<Dto['SmsRecordMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Processing twilio status callback', {
        messageSid: payload.MessageSid,
        messageStatus: payload.MessageStatus
      });
    }

    const smsRecord = await (em ?? this.em).findOneOrFail(
      this.mappers.SmsRecordMapper.entity as typeof SmsRecord,
      { providerMessageId: payload.MessageSid }
    );
    const record = smsRecord as {
      status: SmsStatusEnum;
      error?: string | null;
    };

    record.status = mapTwilioMessageStatus(payload.MessageStatus);
    record.error = payload.ErrorMessage ?? null;

    await (em ?? this.em).flush();

    return this.mappers.SmsRecordMapper.toDto(
      smsRecord as InferEntity<Entities['SmsRecordMapper']>
    );
  }
}
