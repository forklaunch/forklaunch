import { IdDto, IdsDto } from '@forklaunch/common';
import { TtlCache } from '@forklaunch/core/cache';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BasePaymentLinkService } from '@forklaunch/implementation-billing-base/services';
import { PaymentLinkMappers } from '@forklaunch/implementation-billing-base/types';
import { PaymentLinkService } from '@forklaunch/interfaces-billing/interfaces';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { CurrencyEnum, PaymentMethodEnum } from '../domain/enum';
import { StripePaymentLinkMappers } from '../domain/types/paymentLink.mapper.types';
import {
  StripeCreatePaymentLinkDto,
  StripePaymentLinkDto,
  StripePaymentLinkDtos,
  StripeUpdatePaymentLinkDto
} from '../domain/types/stripe.dto.types';
import { StripePaymentLinkEntities } from '../domain/types/stripe.entity.types';

/** Match the required provider object/line-items structure without modeling the SDK. */
function validatePaymentLinkCreateFields(value: unknown): void {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    !Array.isArray((value as Record<string, unknown>).line_items) ||
    !(value as { line_items: unknown[] }).line_items.every(
      (item) =>
        item !== null && typeof item === 'object' && !Array.isArray(item)
    )
  ) {
    throw new Error(
      'Payment link provider options require a line_items array of objects'
    );
  }
}

export class StripePaymentLinkService<
  SchemaValidator extends AnySchemaValidator,
  StatusEnum,
  Entities extends StripePaymentLinkEntities,
  Dto extends
    StripePaymentLinkDtos<StatusEnum> = StripePaymentLinkDtos<StatusEnum>
> implements
    PaymentLinkService<
      PaymentMethodEnum,
      typeof CurrencyEnum,
      StatusEnum,
      {
        CreatePaymentLinkDto: StripeCreatePaymentLinkDto<StatusEnum>;
        UpdatePaymentLinkDto: StripeUpdatePaymentLinkDto<StatusEnum>;
        PaymentLinkDto: StripePaymentLinkDto<StatusEnum>;
        IdDto: IdDto;
        IdsDto: IdsDto;
      }
    >
{
  basePaymentLinkService: BasePaymentLinkService<
    SchemaValidator,
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum,
    Entities,
    Dto
  >;
  private readonly maxConfiguredListRecords: number;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly cache: TtlCache;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: StripePaymentLinkMappers<
    StatusEnum,
    Entities,
    Dto
  >;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    cache: TtlCache,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripePaymentLinkMappers<StatusEnum, Entities, Dto>,
    readonly options?: {
      enableDatabaseBackup?: boolean;
      /** Trusted configured-cache list limit; defaults to 100, maximum 1000. */
      maxConfiguredListRecords?: number;
      telemetry?: TelemetryOptions;
    }
  ) {
    const limit =
      options?.maxConfiguredListRecords === undefined
        ? 100
        : options.maxConfiguredListRecords;
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 1000) {
      throw new Error('Invalid configured payment link list limit');
    }
    this.maxConfiguredListRecords = limit;
    this.stripeClient = stripeClient;
    this.em = em;
    this.cache = cache;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.basePaymentLinkService = new BasePaymentLinkService(
      em,
      cache,
      openTelemetryCollector,
      schemaValidator,
      mappers as PaymentLinkMappers<
        PaymentMethodEnum,
        CurrencyEnum,
        StatusEnum,
        Entities,
        Dto
      >,
      options
    );
  }

  async createPaymentLink(
    paymentLinkDto: StripeCreatePaymentLinkDto<StatusEnum>,
    ...args: unknown[]
  ): Promise<Dto['PaymentLinkMapper']> {
    await this.mappers.CreatePaymentLinkMapper.validateCreate?.(paymentLinkDto);

    validatePaymentLinkCreateFields(paymentLinkDto.stripeFields);
    const session = await this.stripeClient.paymentLinks.create({
      ...paymentLinkDto.stripeFields,
      payment_method_types: paymentLinkDto.paymentMethods,
      currency: paymentLinkDto.currency as string
    });

    await this.mappers.CreatePaymentLinkMapper.validateProviderCreate?.(
      session
    );

    const paymentLink = await this.basePaymentLinkService.createPaymentLink(
      {
        ...paymentLinkDto,
        id: session.id,
        amount:
          paymentLinkDto.amount ??
          session.line_items?.data.reduce<number>(
            (total, item) => total + item.amount_total,
            0
          )
      },
      this.em,
      session,
      ...args
    );

    paymentLink.stripeFields = session;

    return paymentLink;
  }

  /**
   * The Stripe id (`plink_...`) stored with a payment link when it was created.
   * Public ids are local; Stripe only knows its own id. The stored provider
   * object's id wins; a record without one is still usable when its own id is a
   * Stripe id. Anything else fails here rather than sending Stripe a local id.
   */
  protected stripePaymentLinkId(
    paymentLink: Dto['PaymentLinkMapper'],
    id: string
  ): string {
    const stored = (paymentLink?.stripeFields as { id?: unknown } | undefined)
      ?.id;
    const stripeId = stored === undefined ? paymentLink?.id : stored;
    if (
      typeof stripeId !== 'string' ||
      !/^plink_[A-Za-z0-9_]+$/.test(stripeId)
    ) {
      throw new Error(
        `Payment link ${id} has no stored Stripe payment link id`
      );
    }
    return stripeId;
  }

  async updatePaymentLink(
    paymentLinkDto: StripeUpdatePaymentLinkDto<StatusEnum>,
    ...args: unknown[]
  ): Promise<Dto['PaymentLinkMapper']> {
    const existing = await this.basePaymentLinkService.getPaymentLink({
      id: paymentLinkDto.id
    });
    if (this.mappers.UpdatePaymentLinkMapper.validateCachedUpdate) {
      await this.mappers.UpdatePaymentLinkMapper.validateCachedUpdate(
        paymentLinkDto,
        existing
      );
    }
    const stripeId = this.stripePaymentLinkId(existing, paymentLinkDto.id);
    const session = await this.stripeClient.paymentLinks.update(stripeId, {
      ...paymentLinkDto.stripeFields,
      payment_method_types: paymentLinkDto.paymentMethods
    });
    if (session.id !== stripeId) {
      throw new Error('Invalid Stripe billing response');
    }

    const toLocalUpdate = this.mappers.UpdatePaymentLinkMapper.toLocalUpdate;
    if (toLocalUpdate) {
      const patch = await toLocalUpdate(paymentLinkDto, session);
      return this.basePaymentLinkService.updatePaymentLink(
        patch,
        this.em,
        session,
        ...args
      );
    }

    const paymentLink = await this.basePaymentLinkService.updatePaymentLink(
      await this.mappers.PaymentLinkMapper.toDto(
        await this.mappers.UpdatePaymentLinkMapper.toEntity(
          {
            ...paymentLinkDto,
            amount:
              session.line_items?.data.reduce<number>(
                (total, item) => total + item.amount_total,
                0
              ) ?? 0
          },
          this.em,
          session
        )
      ),
      ...args
    );

    paymentLink.stripeFields = session;

    return paymentLink;
  }

  async getPaymentLink({ id }: IdDto): Promise<Dto['PaymentLinkMapper']> {
    const readCachedDto = this.mappers.PaymentLinkMapper.readCachedDto;
    if (readCachedDto) {
      const existing = await this.basePaymentLinkService.getPaymentLink({ id });
      if (existing.id !== id) {
        throw new Error('Cached payment link identity does not match');
      }
      return readCachedDto(existing, (providerId) =>
        this.stripeClient.paymentLinks.retrieve(providerId)
      );
    }
    const stripePaymentLink = await this.stripeClient.paymentLinks.retrieve(id);

    const databasePaymentLink =
      await this.basePaymentLinkService.getPaymentLink({ id });

    databasePaymentLink.stripeFields = stripePaymentLink;

    return databasePaymentLink;
  }

  async expirePaymentLink({ id }: IdDto): Promise<void> {
    const stripeId = this.stripePaymentLinkId(
      await this.basePaymentLinkService.getPaymentLink({ id }),
      id
    );
    await this.stripeClient.paymentLinks.update(stripeId, {
      metadata: {
        status: 'EXPIRED'
      }
    });
    await this.basePaymentLinkService.expirePaymentLink({ id });
  }

  async handlePaymentSuccess({ id }: IdDto): Promise<void> {
    const stripeId = this.stripePaymentLinkId(
      await this.basePaymentLinkService.getPaymentLink({ id }),
      id
    );
    await this.stripeClient.paymentLinks.update(stripeId, {
      metadata: {
        status: 'COMPLETED'
      }
    });
    await this.basePaymentLinkService.handlePaymentSuccess({ id });
  }

  async handlePaymentFailure({ id }: IdDto): Promise<void> {
    const stripeId = this.stripePaymentLinkId(
      await this.basePaymentLinkService.getPaymentLink({ id }),
      id
    );
    await this.stripeClient.paymentLinks.update(stripeId, {
      metadata: {
        status: 'FAILED'
      }
    });
    await this.basePaymentLinkService.handlePaymentFailure({ id });
  }

  async listPaymentLinks(idsDto?: IdsDto): Promise<Dto['PaymentLinkMapper'][]> {
    this.openTelemetryCollector.log('info', idsDto ?? 'idsDto is undefined');
    const { readCachedDto, validateCachedRead } =
      this.mappers.PaymentLinkMapper;
    if (readCachedDto && validateCachedRead) {
      const cached = await this.basePaymentLinkService.listPaymentLinks(
        idsDto,
        this.maxConfiguredListRecords
      );
      for (const value of cached) validateCachedRead(value);
      const active: Dto['PaymentLinkMapper'][] = [];
      for (const value of cached) {
        const fresh = await readCachedDto(value, (providerId) =>
          this.stripeClient.paymentLinks.retrieve(providerId)
        );
        if (typeof fresh.stripeFields.active !== 'boolean') {
          throw new Error('Invalid payment link active state');
        }
        if (fresh.stripeFields.active) active.push(fresh);
      }
      return active;
    }
    const stripePaymentLinks = await this.stripeClient.paymentLinks.list({
      active: true
    });

    const databasePaymentLinks =
      await this.basePaymentLinkService.listPaymentLinks(idsDto);

    return databasePaymentLinks
      .map((paymentLink) => {
        const stripePaymentLink = stripePaymentLinks.data.find(
          (sp) => sp.id === paymentLink.id
        );
        if (!stripePaymentLink) {
          return null;
        }
        paymentLink.stripeFields = stripePaymentLink;
        return paymentLink;
      })
      .filter((paymentLink) => paymentLink !== null);
  }
}
