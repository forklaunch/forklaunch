import { OrderStatus } from '@forklaunch/interfaces-ecommerce/types';
import { EntityManager } from '@mikro-orm/core';
import { Order } from '../../persistence/entities/order.entity';

/**
 * Checkout-idempotency lookup (retry safety). `cartId` on Order
 * associates it with the cart it was checked out from (see order.entity.ts's
 * cartId comment) precisely so a checkout retry — e.g. after payment
 * initiation failed and the client retries with the same, deliberately
 * uncleared cart — can find and reuse that still-PENDING order instead of
 * checkout.controller.ts creating a second order, and a second live
 * provider payment, for the same cart.
 *
 * This lives as its own tiny service (same shape as WebhookEventService),
 * not a BaseOrderService method, because cartId is an ecommerce-stripe-only
 * column: the shared base package's generic Order template
 * (implementations/ecommerce/base) has no cartId field at all, so a
 * cartId-filtered query can't be typed against it — this has to operate on
 * the concrete Order entity directly.
 */
export class OrderCartLookupService {
  constructor(private readonly em: EntityManager) {}

  /**
   * Returns the id of a still-PENDING order for this cart, or null if none
   * exists (the ordinary case — first-time checkout for this cart).
   *
   * Deliberately scoped to status = PENDING, not just cartId: a cart can
   * legitimately be reused for a second, later checkout once the order it
   * produced has moved past PENDING — see order.entity.ts's cartId comment.
   * Matching on cartId alone would wrongly block that legitimate reuse by
   * resurrecting an order that already succeeded (or was cancelled).
   */
  /**
   * The cart an order was checked out from, or null if it has none.
   *
   * The reverse of the lookup below, and needed for the same reason: cartId
   * is an ecommerce-stripe-only column, absent from the shared base package's
   * Order template and therefore from the DTO every service returns. The
   * webhook handler empties the cart once an order is paid and can only get
   * there from the order it was handed.
   */
  async findCartIdByOrderId(orderId: string): Promise<string | null> {
    const order = await this.em.findOne(Order, { id: orderId });
    return order?.cartId ?? null;
  }

  async findPendingOrderIdByCartId(cartId: string): Promise<string | null> {
    const order = await this.em.findOne(Order, {
      cartId,
      status: OrderStatus.PENDING
    });
    return order?.id ?? null;
  }
}
