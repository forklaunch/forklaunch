import { IdDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { SmsService } from '@forklaunch/interfaces-messaging/interfaces';
import { SendSmsDto } from '@forklaunch/interfaces-messaging/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { SmsStatusEnum } from '../domain/enum';
import { BaseSmsDtos } from '../domain/types/baseSmsDto.types';
import { BaseSmsEntities } from '../domain/types/baseSmsEntity.types';
import { SmsMappers } from '../domain/types/sms.mapper.types';
import { SmsRecord } from '../persistence/entities';

export class BaseSmsService<
  SchemaValidator extends AnySchemaValidator,
  StatusEnum,
  MapperEntities extends BaseSmsEntities,
  MapperDomains extends BaseSmsDtos<StatusEnum> = BaseSmsDtos<StatusEnum>
> implements SmsService<StatusEnum> {
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: SmsMappers<StatusEnum, MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: SmsMappers<StatusEnum, MapperEntities, MapperDomains>,
    options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
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
  ): Promise<MapperDomains['SmsRecordMapper']> {
    //! the base implementation does not dispatch to a provider — it logs the
    //! message via the OpenTelemetryCollector and marks the record as sent
    this.openTelemetryCollector.info('Sending sms (base implementation)', {
      to: smsDto.to,
      body: smsDto.body
    });

    const smsRecord = await this.mappers.SendSmsMapper.toEntity(
      smsDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );

    (smsRecord as { status: SmsStatusEnum }).status = SmsStatusEnum.SENT;

    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(smsRecord);
    });

    return this.mappers.SmsRecordMapper.toDto(
      smsRecord as InferEntity<MapperEntities['SmsRecordMapper']>
    );
  }

  async getSmsRecord(
    { id }: IdDto,
    em?: EntityManager
  ): Promise<MapperDomains['SmsRecordMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting sms record', { id });
    }

    const smsRecord = await (em ?? this.em).findOneOrFail(
      this.mappers.SmsRecordMapper.entity as typeof SmsRecord,
      { id }
    );

    return this.mappers.SmsRecordMapper.toDto(
      smsRecord as InferEntity<MapperEntities['SmsRecordMapper']>
    );
  }
}
