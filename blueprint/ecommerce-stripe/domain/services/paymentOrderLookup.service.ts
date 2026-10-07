import { PaymentStatus } from '@forklaunch/interfaces-ecommerce/types';
import { EntityManager } from '@mikro-orm/core';
import { Payment } from '../../persistence/entities/payment.entity';

/**
 * Checkout-idempotency lookup for payments — the payment-side counterpart to
 * OrderCartLookupService, which does the same job one level up for orders.
 *
 * Reusing a pending ORDER on a checkout retry is only half of idempotency.
 * Without this lookup the retry reuses the order and then opens a *second*
 * live PaymentIntent against it, leaving two independently confirmable
 * intents for one order. Both can be confirmed: the order state machine
 * rejects the second PAID transition and worker.ts never decrements
 * inventory twice, so order state and stock both stay correct — the damage
 * is confined to money. The customer is charged twice for a single order,
 * and nothing refunds the surplus.
 *
 * Same tiny-service shape, and the same rationale, as
 * OrderCartLookupService: this queries the concrete Payment entity, and the
 * shared base package's payment services expose no order-scoped query to
 * hang it off.
 */
export class PaymentOrderLookupService {
  constructor(private readonly em: EntityManager) {}

  /**
   * Returns a still-PENDING payment for this order, or null if there is
   * none (the ordinary case — first checkout call for this order).
   *
   * `providerRef` comes back with it because the caller has to route the
   * reuse to the provider that actually created the payment, and the
   * reference is the only record of which one that was.
   *
   * A null providerRef is excluded rather than returned: that payment never
   * reached the provider, so it has no live intent to reuse and poses no
   * double-charge risk. Returning it would be actively harmful — findOne
   * would keep handing back the same dead row on every retry, shadowing the
   * real pending payment this lookup exists to find.
   *
   * Scoped to PENDING deliberately, not just orderId:
   * - SUCCEEDED means the order is already paid. A retry must never be
   *   handed a credential for a charge that has already completed.
   * - FAILED means that attempt is spent — the provider will not accept
   *   another confirmation on it — so a genuine retry needs a fresh one.
   */
  async findPendingPaymentByOrderId(
    orderId: string
  ): Promise<{ id: string; providerRef: string } | null> {
    const payment = await this.em.findOne(Payment, {
      orderId,
      status: PaymentStatus.PENDING,
      providerRef: { $ne: null }
    });
    if (!payment?.providerRef) {
      return null;
    }
    return { id: payment.id, providerRef: payment.providerRef };
  }
}
