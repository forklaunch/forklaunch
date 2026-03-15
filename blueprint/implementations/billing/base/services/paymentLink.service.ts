import { IdDto, IdsDto } from '@forklaunch/common';
import { createCacheKey, TtlCache } from '@forklaunch/core/cache';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { PaymentLinkService } from '@forklaunch/interfaces-billing/interfaces';
import {
  CreatePaymentLinkDto,
  UpdatePaymentLinkDto
} from '@forklaunch/interfaces-billing/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { BasePaymentLinkDtos } from '../domain/types/baseBillingDto.types';
import { BasePaymentLinkEntities } from '../domain/types/baseBillingEntity.types';
import { PaymentLinkMappers } from '../domain/types/paymentLink.mapper.types';

export class BasePaymentLinkService<
  SchemaValidator extends AnySchemaValidator,
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum,
  MapperEntities extends BasePaymentLinkEntities<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >,
  MapperDomains extends BasePaymentLinkDtos<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  > = BasePaymentLinkDtos<PaymentMethodEnum, CurrencyEnum, StatusEnum>
> implements PaymentLinkService<PaymentMethodEnum, CurrencyEnum, StatusEnum>
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  protected enableDatabaseBackup: boolean;
  public em: EntityManager;
  protected cache: TtlCache;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: PaymentLinkMappers<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum,
    MapperEntities,
    MapperDomains
  >;

  constructor(
    em: EntityManager,
    cache: TtlCache,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: PaymentLinkMappers<
      PaymentMethodEnum,
      CurrencyEnum,
      StatusEnum,
      MapperEntities,
      MapperDomains
    >,
    options?: {
      telemetry?: TelemetryOptions;
      enableDatabaseBackup?: boolean;
    }
  ) {
    this.em = em;
    this.cache = cache;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.enableDatabaseBackup = options?.enableDatabaseBackup ?? false;
    this.evaluatedTelemetryOptions = options?.telemetry
      ? evaluateTelemetryOptions(options.telemetry).enabled
      : {
          logging: false,
          metrics: false,
          tracing: false
        };
  }

  protected cacheKeyPrefix = 'payment_link';
  protected createCacheKey = createCacheKey(this.cacheKeyPrefix);

  async createPaymentLink(
    paymentLinkDto: CreatePaymentLinkDto<
      PaymentMethodEnum,
      CurrencyEnum,
      StatusEnum
    >,
    ...args: unknown[]
  ): Promise<MapperDomains['PaymentLinkMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating payment link', paymentLinkDto);
    }

    const paymentLink = await this.mappers.CreatePaymentLinkMapper.toEntity(
      paymentLinkDto,
      args[0] instanceof EntityManager ? args[0] : this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );

    if (this.enableDatabaseBackup) {
      await this.em.persist(paymentLink).flush();
    }

    const createdPaymentLinkDto =
      await this.mappers.PaymentLinkMapper.toDto(paymentLink);

    await this.cache.putRecord({
      key: this.createCacheKey(createdPaymentLinkDto.id),
      value: createdPaymentLinkDto,
      ttlMilliseconds: this.cache.getTtlMilliseconds()
    });

    return createdPaymentLinkDto;
  }

  async updatePaymentLink(
    paymentLinkDto: UpdatePaymentLinkDto<
      PaymentMethodEnum,
      CurrencyEnum,
      StatusEnum
    >,
    ...args: unknown[]
  ): Promise<MapperDomains['PaymentLinkMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating payment link', paymentLinkDto);
    }

    const cacheKey = this.createCacheKey(paymentLinkDto.id);
    const existingLink = (
      await this.cache.readRecord<MapperEntities['PaymentLinkMapper']>(cacheKey)
    )?.value;
    if (!existingLink) {
      throw new Error('Payment link not found');
    }
    const paymentLink = await this.mappers.UpdatePaymentLinkMapper.toEntity(
      paymentLinkDto,
      args[0] instanceof EntityManager ? args[0] : this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );

    if (this.enableDatabaseBackup) {
      await this.em.persist(paymentLink).flush();
    }

    const updatedLinkDto = {
      ...existingLink,
      ...(await this.mappers.PaymentLinkMapper.toDto(paymentLink))
    };

    await this.cache.putRecord({
      key: cacheKey,
      value: updatedLinkDto,
      ttlMilliseconds: this.cache.getTtlMilliseconds()
    });

    return updatedLinkDto;
  }

  async getPaymentLink({
    id
  }: IdDto): Promise<MapperDomains['PaymentLinkMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting payment link', { id });
    }
    const cacheKey = this.createCacheKey(id);
    const paymentLink =
      await this.cache.readRecord<MapperDomains['PaymentLinkMapper']>(cacheKey);
    if (!paymentLink) {
      throw new Error('Payment link not found');
    }

    // Return cached DTO directly - no need to convert to entity and back
    return paymentLink.value;
  }

  async expirePaymentLink({ id }: IdDto): Promise<void> {
    this.openTelemetryCollector.info('Payment link expired', { id });

    if (this.enableDatabaseBackup) {
      const paymentLink = await this.em.upsert(
        this.mappers.PaymentLinkMapper.entity,
        {
          id,
          status: 'EXPIRED'
        }
      );
      await this.em.remove(paymentLink).flush();
    }
    await this.cache.deleteRecord(this.createCacheKey(id));
  }

  async handlePaymentSuccess({ id }: IdDto): Promise<void> {
    this.openTelemetryCollector.info('Payment link success', { id });
    if (this.enableDatabaseBackup) {
      const paymentLink = await this.em.upsert(
        this.mappers.PaymentLinkMapper.entity,
        {
          id,
          status: 'COMPLETED'
        }
      );
      await this.em.remove(paymentLink).flush();
    }
    await this.cache.deleteRecord(this.createCacheKey(id));
  }

  async handlePaymentFailure({ id }: IdDto): Promise<void> {
    this.openTelemetryCollector.info('Payment link failure', { id });
    if (this.enableDatabaseBackup) {
      const paymentLink = await this.em.upsert(
        this.mappers.PaymentLinkMapper.entity,
        {
          id,
          status: 'FAILED'
        }
      );
      await this.em.remove(paymentLink).flush();
    }
    await this.cache.deleteRecord(this.createCacheKey(id));
  }

  async listPaymentLinks(
    idsDto?: IdsDto
  ): Promise<MapperDomains['PaymentLinkMapper'][]> {
    const keys =
      idsDto?.ids && idsDto.ids.length > 0
        ? idsDto?.ids.map((id) => this.createCacheKey(id))
        : await this.cache.listKeys(this.cacheKeyPrefix);

    // Read cached DTOs directly - no need to convert to entity and back
    return Promise.all(
      keys.map(async (key) => {
        const paymentLink =
          await this.cache.readRecord<MapperDomains['PaymentLinkMapper']>(key);
        return paymentLink.value;
      })
    );
  }
}
