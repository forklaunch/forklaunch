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
import { PaypalClient } from './paypal-client';
import { PaypalPaymentMappers } from '../domain/types/payment.mapper.types';

/**
 * Wraps BasePaymentService with real PayPal Orders API calls — same base/
 * provider split as the Stripe implementation. PayPal is the one separate
 * provider build (Venmo rides on it for free). The base does DB persistence;
 * this class calls PayPal and delegates persistence back to base.
 */
export class PaypalPaymentService<
  SchemaValidator extends AnySchemaValidator,
  Entities extends BasePaymentEntities,
  Dto extends BasePaymentDtos = BasePaymentDtos
> implements PaymentService
{
  basePaymentService: BasePaymentService<SchemaValidator, Entities, Dto>;
  protected readonly paypalClient: PaypalClient;
  protected readonly em: EntityManager;

  constructor(
    paypalClient: PaypalClient,
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: PaypalPaymentMappers<Entities, Dto>,
    options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
    this.paypalClient = paypalClient;
    this.em = em;
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

  /** Creates the PayPal order, then persists the pending record with its id as providerRef. */
  async createPayment(
    paymentDto: CreatePaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    const paypalOrder = await this.paypalClient.createOrder({
      amountCents: paymentDto.amountCents,
      currency: paymentDto.currency,
      referenceId: paymentDto.orderId
    });
    return this.basePaymentService.createPayment(
      paymentDto,
      em ?? this.em,
      paypalOrder
    );
  }

  async getPayment(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    return this.basePaymentService.getPayment(idDto, em);
  }

  /**
   * Unlike Stripe's, this confirm has an external side effect: capturing the
   * order moves real money. The base service's guard only protects the DB
   * write, so calling captureOrder before it meant every redelivery of a
   * CHECKOUT.ORDER.APPROVED webhook — routine under at-least-once delivery —
   * re-invoked a live capture.
   *
   * The claim below is an atomic conditional update: only the caller that
   * flips pending -> succeeded captures. A loser (redelivery, or a concurrent
   * delivery that got past the webhook ledger's non-exclusive 'retry' path)
   * affects zero rows, skips the capture, and returns the already-confirmed
   * payment. If the capture then fails we roll the status back so a genuine
   * retry can still claim it.
   */
  async confirmPayment(
    confirmDto: ConfirmPaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    const claimed = await this.basePaymentService.claimForConfirmation(
      confirmDto.providerRef,
      em
    );

    if (!claimed) {
      // Already captured by someone else, or already failed — do not charge
      // again. The base confirm finds by providerRef and skips its write when
      // the status is already succeeded, so this just returns the current DTO
      // (and still throws if the providerRef is genuinely unknown).
      return this.basePaymentService.confirmPayment(confirmDto, em);
    }

    try {
      await this.paypalClient.captureOrder(confirmDto.providerRef);
    } catch (error) {
      await this.basePaymentService.releaseConfirmationClaim(
        confirmDto.providerRef,
        em
      );
      throw error;
    }

    return this.basePaymentService.confirmPayment(confirmDto, em);
  }

  async failPayment(
    failDto: FailPaymentDto,
    em?: EntityManager
  ): Promise<Dto['PaymentMapper']> {
    return this.basePaymentService.failPayment(failDto, em);
  }
}
