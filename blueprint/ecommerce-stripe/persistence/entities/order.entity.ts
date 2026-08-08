import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  OrderItemDto,
  OrderStatus,
  ShippingAddressDto,
  TaxLineDto
} from '@forklaunch/interfaces-ecommerce/types';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { raw } from '@mikro-orm/core';

export const Order = defineComplianceEntity({
  name: 'Order',
  properties: {
    ...sqlBaseProperties,
    customerId: fp.string().nullable().compliance('none'),
    // Set only for orders created via the checkout endpoint (see
    // checkout.controller.ts) — null for orders created directly through
    // order.controller.ts's own createOrder. It's how checkout recognizes
    // "this cart already has an in-flight or just-created PENDING order"
    // on a retry, instead of creating a second order (and a second
    // payment/charge) for the same cart. Not globally unique — a cart can
    // legitimately be reused for a second, later checkout once the first
    // order it produced has moved past PENDING — but the partial unique
    // index below (cartId where status = PENDING) guarantees at most one
    // PENDING order can exist per cart at a time, so two concurrent
    // checkout calls for the same cart (a double-clicked "Pay", or a
    // client retry racing the original) can't both create a live
    // order/payment. See checkout.controller.ts's handling of the
    // resulting UniqueConstraintViolationException.
    cartId: fp.string().nullable().index().compliance('none'),
    status: fp.enum(() => OrderStatus).compliance('none'),
    items: fp.json<OrderItemDto[]>().compliance('none'),
    // A name + street address is real PII — tagged 'pii' (not 'none') so
    // the framework encrypts it at rest automatically, per Guild's own
    // commerce-security guide ("PII — minimize, encrypt at rest").
    shippingAddress: fp.json<ShippingAddressDto>().compliance('pii'),
    subtotalCents: fp.integer().compliance('none'),
    // Promo-code discount, applied before tax — 0 when no code was used.
    discountCents: fp.integer().compliance('none'),
    taxCents: fp.integer().compliance('none'),
    // Per-jurisdiction breakdown, not just the total — the tax-compliance
    // guide is explicit this is required for reconciliation/filing, not a
    // nice-to-have audit trail.
    taxBreakdown: fp.json<TaxLineDto[]>().compliance('none'),
    shippingCents: fp.integer().compliance('none'),
    // Gift-card amount applied against the final total (a tender, not a
    // pre-tax discount) — 0 when no gift card was used.
    giftCardCents: fp.integer().compliance('none'),
    totalCents: fp.integer().compliance('none')
  },
  uniques: [
    {
      name: 'order_cart_id_pending_unique',
      // A partial unique index, not a plain `properties: ['cartId']`
      // unique — cartId is intentionally reusable across orders once the
      // earlier order for that cart has moved past PENDING (see the
      // cartId comment above), so a blanket unique constraint would be
      // wrong. Postgres has no declarative "unique except when X"; a WHERE
      // clause on the index is the standard way to express "unique among
      // PENDING rows only", hence the raw expression here instead of a
      // plain properties-based unique like WebhookEvent's (see
      // webhookEvent.entity.ts).
      expression: (columns, table, indexName) =>
        raw(
          `create unique index ?? on ?? (??) where ?? = '${OrderStatus.PENDING}'`,
          [indexName, table, columns.cartId, columns.status]
        )
    }
  ]
});
