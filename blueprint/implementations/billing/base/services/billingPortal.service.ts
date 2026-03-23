import { IdDto } from '@forklaunch/common';
import { createCacheKey, TtlCache } from '@forklaunch/core/cache';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BillingPortalService } from '@forklaunch/interfaces-billing/interfaces';
import {
  CreateBillingPortalDto,
  UpdateBillingPortalDto
} from '@forklaunch/interfaces-billing/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { BaseBillingDtos } from '../domain/types/baseBillingDto.types';
import { BaseBillingEntities } from '../domain/types/baseBillingEntity.types';
import { BillingPortalMappers } from '../domain/types/billingPortal.mapper.types';

export class BaseBillingPortalService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BaseBillingEntities,
  MapperDomains extends BaseBillingDtos = BaseBillingDtos
> implements BillingPortalService
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
  protected mappers: BillingPortalMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    cache: TtlCache,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: BillingPortalMappers<MapperEntities, MapperDomains>,
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

  protected createCacheKey = createCacheKey('billing_portal_session');

  async createBillingPortalSession(
    billingPortalDto: CreateBillingPortalDto,
    ...args: unknown[]
  ): Promise<MapperDomains['BillingPortalMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Creating billing portal session',
        billingPortalDto
      );
    }

    const billingPortal = await this.mappers.CreateBillingPortalMapper.toEntity(
      billingPortalDto,
      args[0] instanceof EntityManager ? args[0] : this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );

    if (this.enableDatabaseBackup) {
      await this.em.persist(billingPortal).flush();
    }

    const createdBillingPortalDto =
      await this.mappers.BillingPortalMapper.toDto(billingPortal);

    await this.cache.putRecord({
      key: this.createCacheKey(createdBillingPortalDto.id),
      value: createdBillingPortalDto,
      ttlMilliseconds: this.cache.getTtlMilliseconds()
    });

    return createdBillingPortalDto;
  }

  async getBillingPortalSession(
    idDto: IdDto
  ): Promise<MapperDomains['BillingPortalMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting billing portal session', idDto);
    }

    const billingPortalDetails = await this.cache.readRecord<
      MapperDomains['BillingPortalMapper']
    >(this.createCacheKey(idDto.id));

    if (!billingPortalDetails) {
      throw new Error('Session not found');
    }

    return this.mappers.BillingPortalMapper.toDto(
      await this.mappers.UpdateBillingPortalMapper.toEntity(
        billingPortalDetails.value,
        this.em
      )
    );
  }

  async updateBillingPortalSession(
    billingPortalDto: UpdateBillingPortalDto,
    ...args: unknown[]
  ): Promise<MapperDomains['BillingPortalMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Updating billing portal session',
        billingPortalDto
      );
    }

    const existingBillingPortal = (
      await this.cache.readRecord<MapperEntities['BillingPortalMapper']>(
        this.createCacheKey(billingPortalDto.id)
      )
    )?.value;

    if (!existingBillingPortal) {
      throw new Error('Session not found');
    }

    const billingPortal = await this.mappers.UpdateBillingPortalMapper.toEntity(
      billingPortalDto,
      args[0] instanceof EntityManager ? args[0] : this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );

    if (this.enableDatabaseBackup) {
      await this.em.persist(billingPortal).flush();
    }

    const updatedBillingPortalDto = {
      ...existingBillingPortal,
      ...(await this.mappers.BillingPortalMapper.toDto(billingPortal))
    };

    await this.cache.putRecord({
      key: this.createCacheKey(updatedBillingPortalDto.id),
      value: updatedBillingPortalDto,
      ttlMilliseconds: this.cache.getTtlMilliseconds()
    });

    return this.mappers.BillingPortalMapper.toDto(
      await this.mappers.UpdateBillingPortalMapper.toEntity(
        billingPortal,
        this.em
      )
    );
  }

  async expireBillingPortalSession(idDto: IdDto): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Expiring billing portal session',
        idDto
      );
    }

    const sessionExists = await this.cache.readRecord(
      this.createCacheKey(idDto.id)
    );

    if (!sessionExists) {
      throw new Error('Session not found');
    }

    await this.cache.deleteRecord(this.createCacheKey(idDto.id));
  }
}
