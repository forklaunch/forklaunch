import { IdDto } from '@forklaunch/common';
import { TtlCache } from '@forklaunch/core/cache';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BaseCheckoutSessionService } from '@forklaunch/implementation-billing-base/services';
import { CheckoutSessionMappers } from '@forklaunch/implementation-billing-base/types';
import { CheckoutSessionService } from '@forklaunch/interfaces-billing/interfaces';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { CurrencyEnum } from '../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../domain/enum/paymentMethod.enum';
import { StripeCheckoutSessionMappers } from '../domain/types/checkoutSession.mapper.types';
import {
  StripeCheckoutSessionDtos,
  StripeCreateCheckoutSessionDto
} from '../domain/types/stripe.dto.types';
import { StripeCheckoutSessionEntities } from '../domain/types/stripe.entity.types';

export class StripeCheckoutSessionService<
  SchemaValidator extends AnySchemaValidator,
  StatusEnum,
  MapperEntities extends StripeCheckoutSessionEntities,
  Dto extends StripeCheckoutSessionDtos<StatusEnum> =
    StripeCheckoutSessionDtos<StatusEnum>
> implements CheckoutSessionService<
  typeof PaymentMethodEnum,
  typeof CurrencyEnum,
  StatusEnum,
  {
    CreateCheckoutSessionDto: Dto['CreateCheckoutSessionMapper'];
    CheckoutSessionDto: Dto['CheckoutSessionMapper'];
    IdDto: IdDto;
  }
> {
  baseCheckoutSessionService: BaseCheckoutSessionService<
    SchemaValidator,
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum,
    MapperEntities,
    Dto
  >;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly cache: TtlCache;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: StripeCheckoutSessionMappers<
    StatusEnum,
    MapperEntities,
    Dto
  >;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    cache: TtlCache,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripeCheckoutSessionMappers<StatusEnum, MapperEntities, Dto>,
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
    this.baseCheckoutSessionService = new BaseCheckoutSessionService(
      em,
      cache,
      openTelemetryCollector,
      schemaValidator,
      mappers as CheckoutSessionMappers<
        PaymentMethodEnum,
        CurrencyEnum,
        StatusEnum,
        MapperEntities,
        Dto
      >,
      options
    );
  }

  async createCheckoutSession(
    checkoutSessionDto: StripeCreateCheckoutSessionDto<StatusEnum>,
    ...args: unknown[]
  ): Promise<Dto['CheckoutSessionMapper']> {
    await this.mappers.CreateCheckoutSessionMapper.validateCreate?.(checkoutSessionDto);

    const session = await this.stripeClient.checkout.sessions.create({
      ...checkoutSessionDto.stripeFields,
      mode: 'subscription',
      payment_method_types: checkoutSessionDto.paymentMethods,
      currency: checkoutSessionDto.currency as string,
      success_url: checkoutSessionDto.successRedirectUri,
      cancel_url: checkoutSessionDto.cancelRedirectUri
    });

    await this.mappers.CreateCheckoutSessionMapper.validateProviderCreate?.(session);

    return await this.baseCheckoutSessionService.createCheckoutSession(
      {
        ...checkoutSessionDto,
        id: session.id,
        uri: session.url ?? undefined,
        expiresAt: new Date(Date.now() + 5 * 60 * 1000),
        providerFields: session
      },
      this.em,
      session,
      ...args
    );
  }

  async getCheckoutSession(
    idDto: IdDto
  ): Promise<Dto['CheckoutSessionMapper']> {
    const readCachedDto = this.mappers.CheckoutSessionMapper.readCachedDto;
    if (readCachedDto) {
      const existing =
        await this.baseCheckoutSessionService.getCheckoutSession(idDto);
      if (existing.id !== idDto.id) {
        throw new Error('Cached checkout identity does not match');
      }
      return readCachedDto(existing, (providerId) =>
        this.stripeClient.checkout.sessions.retrieve(providerId)
      );
    }
    const session = await this.stripeClient.checkout.sessions.retrieve(
      idDto.id
    );
    const databaseCheckoutSession =
      await this.baseCheckoutSessionService.getCheckoutSession(idDto);

    databaseCheckoutSession.stripeFields = session;

    return databaseCheckoutSession;
  }

  /**
   * The Stripe id (`cs_...`) stored with a checkout session when it was
   * created. Public ids are local; Stripe only knows its own id. The stored
   * provider object's id wins; a record without one is still usable when its
   * own id is a Stripe id. Anything else fails here rather than sending Stripe
   * a local id.
   */
  protected async stripeCheckoutSessionId(id: string): Promise<string> {
    const checkoutSession =
      await this.baseCheckoutSessionService.getCheckoutSession({ id });
    const stored = (
      checkoutSession?.stripeFields as { id?: unknown } | undefined
    )?.id;
    const stripeId = stored === undefined ? checkoutSession?.id : stored;
    if (typeof stripeId !== 'string' || !/^cs_[A-Za-z0-9_]+$/.test(stripeId)) {
      throw new Error(
        `Checkout session ${id} has no stored Stripe checkout session id`
      );
    }
    return stripeId;
  }

  async expireCheckoutSession({ id }: IdDto): Promise<void> {
    await this.stripeClient.checkout.sessions.expire(
      await this.stripeCheckoutSessionId(id)
    );
    await this.baseCheckoutSessionService.expireCheckoutSession({ id });
  }

  async handleCheckoutSuccess({ id }: IdDto): Promise<void> {
    await this.stripeClient.checkout.sessions.update(
      await this.stripeCheckoutSessionId(id),
      {
        metadata: {
          status: 'SUCCESS'
        }
      }
    );
    await this.baseCheckoutSessionService.handleCheckoutSuccess({ id });
  }

  async handleCheckoutFailure({ id }: IdDto): Promise<void> {
    await this.stripeClient.checkout.sessions.update(
      await this.stripeCheckoutSessionId(id),
      {
        metadata: {
          status: 'FAILED'
        }
      }
    );
    await this.baseCheckoutSessionService.handleCheckoutFailure({ id });
  }
}
