import { IdDto } from '@forklaunch/common';
import { createCacheKey, TtlCache } from '@forklaunch/core/cache';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { CheckoutSessionService } from '@forklaunch/interfaces-billing/interfaces';
import { CreateCheckoutSessionDto } from '@forklaunch/interfaces-billing/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { CheckoutSession } from '../persistence/entities';
import { BaseCheckoutSessionDtos } from '../domain/types/baseBillingDto.types';
import { BaseCheckoutSessionEntities } from '../domain/types/baseBillingEntity.types';
import { CheckoutSessionMappers } from '../domain/types/checkoutSession.mapper.types';

export class BaseCheckoutSessionService<
  SchemaValidator extends AnySchemaValidator,
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum,
  MapperEntities extends BaseCheckoutSessionEntities<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >,
  MapperDomains extends BaseCheckoutSessionDtos<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  > = BaseCheckoutSessionDtos<PaymentMethodEnum, CurrencyEnum, StatusEnum>
> implements CheckoutSessionService<PaymentMethodEnum, CurrencyEnum, StatusEnum>
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
  protected mappers: CheckoutSessionMappers<
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
    mappers: CheckoutSessionMappers<
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

  protected createCacheKey = createCacheKey('checkout_session');

  async createCheckoutSession(
    checkoutSessionDto: CreateCheckoutSessionDto<
      PaymentMethodEnum,
      CurrencyEnum,
      StatusEnum
    >,
    ...args: unknown[]
  ): Promise<MapperDomains['CheckoutSessionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Creating checkout session',
        checkoutSessionDto
      );
    }

    const checkoutSession =
      await this.mappers.CreateCheckoutSessionMapper.toEntity(
        checkoutSessionDto,
        args[0] instanceof EntityManager ? args[0] : this.em,
        ...(args[0] instanceof EntityManager ? args.slice(1) : args)
      );

    const createdCheckoutSessionDto =
      await this.mappers.CheckoutSessionMapper.toDto(checkoutSession);

    if (this.enableDatabaseBackup) {
      await this.em.persist(checkoutSession).flush();
    }

    await this.cache.putRecord({
      key: this.createCacheKey(createdCheckoutSessionDto.id),
      value: createdCheckoutSessionDto,
      ttlMilliseconds: this.cache.getTtlMilliseconds()
    });

    return createdCheckoutSessionDto;
  }

  async getCheckoutSession({
    id
  }: IdDto): Promise<MapperDomains['CheckoutSessionMapper']> {
    const checkoutSessionDetails = await this.cache.readRecord<
      MapperDomains['CheckoutSessionMapper']
    >(this.createCacheKey(id));
    if (!checkoutSessionDetails) {
      throw new Error('Session not found');
    }

    return this.mappers.CheckoutSessionMapper.toDto(
      await this.mappers.UpdateCheckoutSessionMapper.toEntity(
        checkoutSessionDetails.value,
        this.em
      )
    );
  }

  async expireCheckoutSession({ id }: IdDto): Promise<void> {
    const checkoutSessionDetails = await this.cache.readRecord(
      this.createCacheKey(id)
    );
    if (!checkoutSessionDetails) {
      throw new Error('Session not found');
    }
    await this.cache.deleteRecord(this.createCacheKey(id));
  }

  async handleCheckoutSuccess({ id }: IdDto): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Checkout success', { id });
    }

    if (this.enableDatabaseBackup) {
      const checkoutSession = await this.em.upsert(
        this.mappers.CheckoutSessionMapper.entity as typeof CheckoutSession,
        {
          id,
          status: 'SUCCESS'
        }
      );
      await this.em.persist(checkoutSession).flush();
    }

    await this.cache.deleteRecord(this.createCacheKey(id));
  }

  async handleCheckoutFailure({ id }: IdDto): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Checkout failure', { id });
    }

    if (this.enableDatabaseBackup) {
      const checkoutSession = await this.em.upsert(
        this.mappers.CheckoutSessionMapper.entity as typeof CheckoutSession,
        {
          id,
          status: 'FAILED'
        }
      );
      await this.em.persist(checkoutSession).flush();
    }

    await this.cache.deleteRecord(this.createCacheKey(id));
  }
}
