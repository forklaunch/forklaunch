import { Migration } from '@mikro-orm/migrations';

export class Migration20260826010000 extends Migration {
  override name = 'Migration20260826010000';

  override up(): void | Promise<void> {
    // At most one live payment per order. checkout.controller.ts looks for an
    // existing pending payment before creating one, but that lookup is a plain
    // findOne rather than a lock: two concurrent checkouts for the same cart —
    // a double-clicked "Pay", or a client retry racing the original — can both
    // read no-pending-payment and both open a live provider payment. Two
    // confirmable payments for one order means the customer can be charged
    // twice, and nothing downstream catches it: the order state machine
    // rejects the second PAID transition and the worker never decrements stock
    // twice, so order state and inventory both stay correct and the duplicate
    // surfaces only as money taken.
    //
    // Partial on two counts, and both have to match
    // PaymentOrderLookupService's query exactly:
    //   status = 'pending'      — an order legitimately gets a second payment
    //                             once the first has FAILED, and a blanket
    //                             unique on order_id would block that retry
    //                             forever.
    //   provider_ref not null   — a pending payment that never reached the
    //                             provider holds no live intent and poses no
    //                             double-charge risk. Constraining it anyway
    //                             would let such a row occupy the slot while
    //                             staying invisible to the lookup, wedging
    //                             checkout on that order permanently: the
    //                             lookup finds nothing, the insert violates
    //                             this index, the race-winner lookup finds
    //                             nothing either, and every retry 502s.
    //
    // Mirrors order_cart_id_pending_unique (see the initial migration), which
    // closes the same race one level up for orders.
    this.addSql(
      `create unique index "payment_order_id_pending_unique" on "public"."payment" ("order_id") where "status" = 'pending' and "provider_ref" is not null;`
    );
  }

  override down(): void | Promise<void> {
    this.addSql(`drop index "public"."payment_order_id_pending_unique";`);
  }
}
