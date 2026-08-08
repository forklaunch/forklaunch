import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BasePaymentService } from '@forklaunch/implementation-ecommerce-base/services';
import {
  BasePaymentDtos,
  BasePaymentEntities
} from '@forklaunch/implementation-ecommerce-base/types';
import { PaymentService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  ConfirmPaymentDto,
  CreatePaymentDto,
  FailPaymentDto
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePaymentMappers } from '../domain/types/payment.mapper.types';

/**
 * Wraps BasePaymentService with real Stripe PaymentIntent calls — the base
 * does DB persistence, this class calls the provider API and delegates
 * persistence back to base (same split as StripePlanService/BasePlanService
 * in billing). Fills the ECOM-10 payment seam: PaymentIntent drives the order
 * toward paid; confirmation is idempotent and driven by the webhook.
 */
export class StripePaymentService<
  SchemaValidator extends AnySchemaValidator,
  Entities extends BasePaymentEntities,
  Dto extends BasePaymentDtos = BasePaymentDtos
> implements PaymentService
{
  basePaymentService: BasePaymentService<SchemaValidator, Entities, Dto>;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripePaymentMappers<Entities, Dto>,
    options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
    this.stripeClient = stripeClient;
    this.em = em;
    // CreatePaymentMapper.toEntity's 3rd param is concretely Stripe.PaymentIntent
    // here (narrower than base's generic ...args: unknown[]) — safe at runtime,
    // this cast just satisfies the base constructor's looser declared type.
    this.basePaymentService = new BasePaymentService(
      em,
      openTelemetryCollector,
      schemaValidator,
      mappers as unknown as ConstructorParameters<
        typeof BasePaymentService<SchemaValidator, Entities, Dto>
      >[3],
      options
    );
  }

  /** Creates the Stripe PaymentIntent, then persists the pending record with its id as providerRef. */
  async createPayment(
    paymentDto: CreatePaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    const paymentIntent = await this.stripeClient.paymentIntents.create({
      amount: paymentDto.amountCents,
      currency: paymentDto.currency,
      metadata: { orderId: paymentDto.orderId }
    });
    return this.basePaymentService.createPayment(
      paymentDto,
      em ?? this.em,
      paymentIntent
    );
  }

  async getPayment(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    return this.basePaymentService.getPayment(idDto, em);
  }

  /** Driven by the Stripe webhook (signature-verified at the controller layer) — idempotent. */
  async confirmPayment(
    confirmDto: ConfirmPaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    return this.basePaymentService.confirmPayment(confirmDto, em);
  }

  /** Failed payment emits the event dunning (ECOM-20) hooks into, at the deployable-app layer. */
  async failPayment(
    failDto: FailPaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    return this.basePaymentService.failPayment(failDto, em);
  }
}
