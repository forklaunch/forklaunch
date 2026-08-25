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
 * in billing). Fills the payment seam: PaymentIntent drives the order
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
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly connect?: {
    connectedAccountId: string;
    platformFeeBps: number;
  };

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripePaymentMappers<Entities, Dto>,
    options?: {
      telemetry?: TelemetryOptions;
      /**
       * Stripe Connect (platform) mode. When set, PaymentIntents are created
       * as DIRECT charges on the merchant's connected account — the merchant
       * is the merchant of record and settles the funds — with
       * `application_fee_amount` computed from platformFeeBps. The launch
       * business model is NO markup (fee 0, see the Guild deck: "payments
       * flat at launch"), so platformFeeBps defaults to 0 and the fee is
       * omitted entirely; it exists as a dial, not a revenue assumption.
       * When absent, behavior is unchanged: the deploy's own STRIPE_API_KEY
       * account is the merchant (bring-your-own-key mode).
       */
      connect?: {
        connectedAccountId: string;
        platformFeeBps?: number;
      };
    }
  ) {
    this.stripeClient = stripeClient;
    this.em = em;
    this.openTelemetryCollector = openTelemetryCollector;
    if (options?.connect) {
      // Validate the fee dial here rather than trusting the caller: it comes
      // from an env var, and every bad value fails SILENTLY downstream
      // otherwise. A non-numeric STRIPE_PLATFORM_FEE_BPS makes Number()
      // return NaN, `feeCents > 0` is then false, and the application fee is
      // quietly omitted — an operator who meant to charge a fee would get
      // none, with nothing logged. A negative value does the same. Failing
      // at construction turns a silent misconfiguration into a startup error.
      const platformFeeBps = options.connect.platformFeeBps ?? 0;
      // Upper bound is exclusive: Stripe requires application_fee_amount to be
      // strictly less than the charge, so 10000 bps (the whole charge) is never
      // a usable value and is rejected here rather than at the first checkout.
      if (
        !Number.isInteger(platformFeeBps) ||
        platformFeeBps < 0 ||
        platformFeeBps >= 10000
      ) {
        throw new Error(
          `Invalid Stripe platform fee: expected an integer from 0 to 9999 basis points, got ${platformFeeBps}. ` +
            'Check STRIPE_PLATFORM_FEE_BPS.'
        );
      }
      this.connect = {
        connectedAccountId: options.connect.connectedAccountId,
        platformFeeBps
      };
    }
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

  /**
   * Creates the Stripe PaymentIntent, then persists the pending record with
   * its id as providerRef. The PaymentIntent's `client_secret` is bolted
   * onto the returned DTO (never persisted — Payment has no clientSecret
   * column, deliberately: it's a one-time, provider-issued credential the
   * frontend needs *right now* to actually collect the charge via
   * Stripe.js, not something that should still be fetchable from GET
   * /payment/:id later). This widens the return type over the
   * PaymentService interface's `Promise<Params['PaymentDto']>` — a covariant,
   * backward-compatible narrowing (an intersection type is always assignable
   * to its base), the same relationship StripePaymentMappers already has to
   * the base PaymentMappers for this class's mapper argument.
   */
  async createPayment(
    paymentDto: CreatePaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper'] & { clientSecret?: string }> {
    // In Connect mode this is a DIRECT charge: created on the merchant's
    // connected account (stripeAccount request option), so the merchant is
    // merchant-of-record and keeps the funds. The platform fee is only
    // attached when non-zero — at launch it always is zero (no markup).
    const requestedFeeCents = this.connect
      ? Math.round(
          (paymentDto.amountCents * this.connect.platformFeeBps) / 10000
        )
      : 0;
    // application_fee_amount has to stay strictly below the charge. The
    // constructor already rejects 10000 bps, but rounding can still reach the
    // full amount on a small enough charge (1 cent at 9999 bps rounds to 1),
    // and Stripe would reject the PaymentIntent outright. Cap it so a fee
    // setting cannot fail a customer's checkout, and record it when it bites.
    const feeCents = Math.min(
      requestedFeeCents,
      Math.max(paymentDto.amountCents - 1, 0)
    );
    if (feeCents !== requestedFeeCents) {
      this.openTelemetryCollector.warn(
        'Stripe platform fee capped below the charge amount',
        {
          orderId: paymentDto.orderId,
          amountCents: paymentDto.amountCents,
          requestedFeeCents,
          appliedFeeCents: feeCents
        }
      );
    }
    const paymentIntent = await this.stripeClient.paymentIntents.create(
      {
        amount: paymentDto.amountCents,
        currency: paymentDto.currency,
        metadata: { orderId: paymentDto.orderId },
        ...(this.connect && feeCents > 0
          ? { application_fee_amount: feeCents }
          : {})
      },
      this.connect
        ? { stripeAccount: this.connect.connectedAccountId }
        : undefined
    );
    const payment = await this.basePaymentService.createPayment(
      paymentDto,
      em ?? this.em,
      paymentIntent
    );
    return {
      ...payment,
      clientSecret: paymentIntent.client_secret ?? undefined
    };
  }

  /**
   * Re-issues the client_secret for a payment that already has a live
   * PaymentIntent, so a checkout retry resumes that intent instead of
   * opening a second one against the same order. Two live intents for one
   * order are both confirmable, and a customer who confirms both is charged
   * twice — see PaymentOrderLookupService in ecommerce-stripe for the full
   * failure mode.
   *
   * The secret has to be fetched back from Stripe because it is
   * deliberately never persisted (see createPayment) — Stripe is the only
   * place it exists after that first response.
   *
   * A payment with no providerRef never reached Stripe, so there is no
   * intent to resume and the record is returned unchanged, without a
   * secret. The caller treats that the same way it treats no payment at
   * all: create a fresh one.
   */
  async resumePayment(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<Dto['PaymentMapper'] & { clientSecret?: string }> {
    const payment = await this.basePaymentService.getPayment(idDto, em);
    if (!payment.providerRef) {
      return payment;
    }
    const paymentIntent = await this.stripeClient.paymentIntents.retrieve(
      payment.providerRef,
      {},
      this.connect
        ? { stripeAccount: this.connect.connectedAccountId }
        : undefined
    );
    return {
      ...payment,
      clientSecret: paymentIntent.client_secret ?? undefined
    };
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

  /** Failed payment emits the event dunning hooks into, at the deployable-app layer. */
  async failPayment(
    failDto: FailPaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    return this.basePaymentService.failPayment(failDto, em);
  }
}
