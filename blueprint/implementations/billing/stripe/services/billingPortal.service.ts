import { IdDto } from '@forklaunch/common';
import { TtlCache } from '@forklaunch/core/cache';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BaseBillingPortalService } from '@forklaunch/implementation-billing-base/services';
import { BillingPortalMappers } from '@forklaunch/implementation-billing-base/types';
import { BillingPortalService } from '@forklaunch/interfaces-billing/interfaces';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeBillingPortalMappers } from '../domain/types/billingPortal.mapper.types';
import {
  StripeBillingPortalDtos,
  StripeCreateBillingPortalDto,
  StripeUpdateBillingPortalDto
} from '../domain/types/stripe.dto.types';
import { StripeBillingPortalEntities } from '../domain/types/stripe.entity.types';

export class StripeBillingPortalService<
  SchemaValidator extends AnySchemaValidator,
  Entities extends StripeBillingPortalEntities,
  Dto extends StripeBillingPortalDtos = StripeBillingPortalDtos
> implements BillingPortalService<{
  CreateBillingPortalDto: Dto['CreateBillingPortalMapper'];
  UpdateBillingPortalDto: Dto['UpdateBillingPortalMapper'];
  BillingPortalDto: Dto['BillingPortalMapper'];
  IdDto: IdDto;
}> {
  baseBillingPortalService: BaseBillingPortalService<
    SchemaValidator,
    Entities,
    Dto
  >;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly cache: TtlCache;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  private readonly billingPortalSessionExpiryDurationMs = 5 * 60 * 1000;
  protected readonly mappers: StripeBillingPortalMappers<Entities, Dto>;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    cache: TtlCache,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripeBillingPortalMappers<Entities, Dto>,
    readonly options?: {
      enableDatabaseBackup?: boolean;
      telemetry?: TelemetryOptions;
    }
  ) {
    this.stripeClient = stripeClient;
    this.em = em;
    this.cache = cache;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.baseBillingPortalService = new BaseBillingPortalService<
      SchemaValidator,
      Entities,
      Dto
    >(
      em,
      cache,
      openTelemetryCollector,
      schemaValidator,
      mappers as BillingPortalMappers<Entities, Dto>,
      options
    );
  }

  async createBillingPortalSession(
    billingPortalDto: StripeCreateBillingPortalDto,
    ...args: unknown[]
  ): Promise<Dto['BillingPortalMapper']> {
    const session = await this.stripeClient.billingPortal.sessions.create({
      ...billingPortalDto.stripeFields,
      customer: billingPortalDto.customerId
    });

    return await this.baseBillingPortalService.createBillingPortalSession(
      {
        ...billingPortalDto,
        id: session.id,
        uri: session.url,
        expiresAt: new Date(
          Date.now() + this.billingPortalSessionExpiryDurationMs
        )
      },
      this.em,
      session,
      ...args
    );
  }

  async getBillingPortalSession(
    idDto: IdDto
  ): Promise<Dto['BillingPortalMapper']> {
    return await this.baseBillingPortalService.getBillingPortalSession(idDto);
  }

  async expireBillingPortalSession(idDto: IdDto): Promise<void> {
    return this.baseBillingPortalService.expireBillingPortalSession(idDto);
  }

  async updateBillingPortalSession(
    billingPortalDto: StripeUpdateBillingPortalDto,
    ...args: unknown[]
  ): Promise<Dto['BillingPortalMapper']> {
    const existingSession =
      await this.baseBillingPortalService.getBillingPortalSession({
        id: billingPortalDto.id
      });
    const session = await this.stripeClient.billingPortal.sessions.create({
      ...billingPortalDto.stripeFields,
      customer: billingPortalDto.customerId || existingSession.customerId
    });

    return await this.baseBillingPortalService.updateBillingPortalSession(
      {
        ...billingPortalDto,
        id: existingSession.id, // Use the original database ID, not the new Stripe session ID
        uri: session.url,
        expiresAt: new Date(
          Date.now() + this.billingPortalSessionExpiryDurationMs
        )
      },
      this.em,
      session,
      ...args
    );
  }
}
