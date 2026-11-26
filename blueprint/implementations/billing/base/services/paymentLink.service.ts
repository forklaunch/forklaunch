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
import { PaymentLink } from '../persistence/entities';
import { BasePaymentLinkDtos } from '../domain/types/baseBillingDto.types';
import { BasePaymentLinkEntities } from '../domain/types/baseBillingEntity.types';
import { PaymentLinkMappers } from '../domain/types/paymentLink.mapper.types';

export class BasePaymentLinkService<
  SchemaValidator extends AnySchemaValidator,
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum,
  MapperEntities extends BasePaymentLinkEntities,
  MapperDomains extends BasePaymentLinkDtos<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  > = BasePaymentLinkDtos<PaymentMethodEnum, CurrencyEnum, StatusEnum>
> implements PaymentLinkService<PaymentMethodEnum, CurrencyEnum, StatusEnum> {
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
      await this.cache.readRecord<MapperDomains['PaymentLinkMapper']>(cacheKey)
    )?.value;
    if (!existingLink) {
      throw new Error('Payment link not found');
    }
    if (
      !this.enableDatabaseBackup &&
      this.mappers.UpdatePaymentLinkMapper.toCachedDto
    ) {
      if (existingLink.id !== paymentLinkDto.id) {
        throw new Error('Cached payment link identity cannot change');
      }
      const updated = await this.mappers.UpdatePaymentLinkMapper.toCachedDto(
        paymentLinkDto,
        existingLink,
        args[0] instanceof EntityManager ? args[0] : this.em,
        ...(args[0] instanceof EntityManager ? args.slice(1) : args)
      );
      if (updated.id !== paymentLinkDto.id) {
        throw new Error('Cached payment link identity cannot change');
      }
      await this.cache.putRecord({
        key: cacheKey,
        value: updated,
        ttlMilliseconds: this.cache.getTtlMilliseconds()
      });
      return updated;
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

  private normalizeCachedPaymentLink(
    value: MapperDomains['PaymentLinkMapper'],
    cacheKey: string
  ): MapperDomains['PaymentLinkMapper'] {
    const normalize = this.mappers.PaymentLinkMapper.normalizeCachedDto;
    if (!normalize) return value;
    if (
      !value ||
      typeof value.id !== 'string' ||
      !value.id.trim() ||
      this.createCacheKey(value.id) !== cacheKey
    ) {
      throw new Error('Cached payment link identity does not match');
    }
    const normalized = normalize(value);
    if (normalized.id !== value.id) {
      throw new Error('Cached payment link identity cannot change');
    }
    return normalized;
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

    return this.normalizeCachedPaymentLink(paymentLink.value, cacheKey);
  }

  async expirePaymentLink({ id }: IdDto): Promise<void> {
    this.openTelemetryCollector.info('Payment link expired', { id });

    if (this.enableDatabaseBackup) {
      const paymentLink = await this.em.upsert(
        this.mappers.PaymentLinkMapper.entity as typeof PaymentLink,
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
        this.mappers.PaymentLinkMapper.entity as typeof PaymentLink,
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
        this.mappers.PaymentLinkMapper.entity as typeof PaymentLink,
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
    idsDto?: IdsDto,
    maxValidatedRecords?: number
  ): Promise<MapperDomains['PaymentLinkMapper'][]> {
    if (
      maxValidatedRecords !== undefined &&
      (!Number.isSafeInteger(maxValidatedRecords) ||
        maxValidatedRecords < 1 ||
        (idsDto?.ids?.length ?? 0) > maxValidatedRecords)
    ) {
      throw new Error('Configured payment link list exceeds its record limit');
    }
    if (
      maxValidatedRecords !== undefined &&
      idsDto?.ids?.some((id) => typeof id !== 'string' || !id.trim())
    ) {
      throw new Error('Invalid configured payment link identity');
    }
    const keys =
      idsDto?.ids && idsDto.ids.length > 0
        ? idsDto?.ids.map((id) => this.createCacheKey(id))
        : await this.cache.listKeys(this.cacheKeyPrefix);

    if (
      maxValidatedRecords !== undefined &&
      keys.length > maxValidatedRecords
    ) {
      throw new Error('Configured payment link list exceeds its record limit');
    }
    // The opt-in bound also validates identities before provider reads.
    return Promise.all(
      keys.map(async (key) => {
        const paymentLink =
          await this.cache.readRecord<MapperDomains['PaymentLinkMapper']>(key);
        if (
          maxValidatedRecords !== undefined &&
          (!paymentLink?.value ||
            typeof paymentLink.value.id !== 'string' ||
            !paymentLink.value.id.trim() ||
            this.createCacheKey(paymentLink.value.id) !== key)
        ) {
          throw new Error(
            'Cached payment link is missing or its identity does not match'
          );
        }
        return this.normalizeCachedPaymentLink(paymentLink.value, key);
      })
    );
  }
}
