import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { PaymentStatus } from '@forklaunch/interfaces-ecommerce/types';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { raw } from '@mikro-orm/core';

export const Payment = defineComplianceEntity({
  name: 'Payment',
  properties: {
    ...sqlBaseProperties,
    orderId: fp.string().compliance('none'),
    amountCents: fp.integer().compliance('none'),
    currency: fp.string().compliance('none'),
    status: fp.enum(() => PaymentStatus).compliance('none'),
    // Stripe PaymentIntent id — never raw card data (Stripe holds that).
    providerRef: fp.string().nullable().unique().compliance('none')
  },
  uniques: [
    {
      name: 'payment_order_id_pending_unique',
      // At most one PENDING payment per order. checkout.controller.ts looks
      // for an existing pending payment before creating one, but that lookup
      // is a plain findOne rather than a lock: two concurrent checkouts for
      // the same cart — a double-clicked "Pay", or a client retry racing the
      // original — can both read no-pending-payment and both go on to open a
      // live PaymentIntent. Two confirmable intents for one order means the
      // customer can be charged twice, and neither the order state machine
      // nor the worker's conditional decrement catches that: both correctly
      // treat the order as paid exactly once, so the duplicate shows up only
      // as money taken.
      //
      // Partial rather than a blanket unique on orderId for the same reason
      // Order's is (see order.entity.ts): an order legitimately gets a second
      // payment once the first has FAILED, and a plain unique would block
      // that retry forever.
      //
      // The `providerRef is not null` half of the predicate matters just as
      // much, and has to stay in step with PaymentOrderLookupService's query,
      // which excludes null-ref rows for its own reasons. A pending payment
      // with no provider reference never reached the provider, so it holds no
      // live intent and creates no double-charge risk — it is not what this
      // index exists to prevent. Constraining it anyway would let such a row
      // occupy the slot while staying invisible to the lookup, and checkout
      // would then wedge permanently on that order: the lookup finds nothing,
      // the insert violates the index, the race-winner lookup finds nothing
      // either, and every retry 502s forever.
      expression: (columns, table, indexName) =>
        raw(
          `create unique index ?? on ?? (??) where ?? = '${PaymentStatus.PENDING}' and ?? is not null`,
          [
            indexName,
            table,
            columns.orderId,
            columns.status,
            columns.providerRef
          ]
        )
    }
  ]
});
