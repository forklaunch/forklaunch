import {
  enum_,
  handlers,
  optional,
  schemaValidator,
  string
} from '../../schema';
import {
  CheckoutResultSchema,
  ShippingAddressSchema
} from '../../domain/schemas/checkout.schema';
import { PaymentProvider } from '../../domain/enum/paymentProvider.enum';
import { ci, tokens } from '../../bootstrapper';
import { UniqueConstraintViolationException } from '@mikro-orm/core';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const cartServiceFactory = ci.scopedResolver(tokens.CartService);
const variantServiceFactory = ci.scopedResolver(tokens.VariantService);
const inventoryServiceFactory = ci.scopedResolver(tokens.InventoryService);
const orderServiceFactory = ci.scopedResolver(tokens.OrderService);
const orderCartLookupServiceFactory = ci.scopedResolver(
  tokens.OrderCartLookupService
);
const paymentOrderLookupServiceFactory = ci.scopedResolver(
  tokens.PaymentOrderLookupService
);
const taxServiceFactory = ci.scopedResolver(tokens.TaxService);
const shippingServiceFactory = ci.scopedResolver(tokens.ShippingService);
const stripePaymentServiceFactory = ci.scopedResolver(tokens.PaymentService);
const paypalPaymentServiceFactory = ci.scopedResolver(
  tokens.PaypalPaymentService
);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

/**
 * All amounts in this module are USD cents — matches the existing
 * assumption elsewhere in this checkout flow (StripeTaxService hardcodes
 * `currency: 'usd'` for its tax.calculations.create call; nothing upstream
 * of here — cart, variant pricing, order — carries a currency field at
 * all). Not introduced by this change, just made explicit at the one new
 * call site (payment creation) that also needs a currency.
 */
const CHECKOUT_CURRENCY = 'usd';

/**
 * Stripe object-id prefix for a PaymentIntent. Used to tell which provider
 * an existing payment belongs to, since Payment has no provider column —
 * see the payment-reuse block below.
 */
const STRIPE_PAYMENT_INTENT_PREFIX = 'pi_';

/**
 * The unified checkout orchestration: cart -> order -> payment
 * in one call, with stock validated up front so a customer never pays for
 * something that's not actually there. Tax (Stripe Tax) and shipping
 * (flat-rate) both run for real, which is why a shipping address is required
 * because both need one.
 *
 * Order creation and payment creation are two separate calls (Payment.
 * orderId is required, so the order must exist first) — see the payment
 * try/catch below for what happens if the second one fails.
 *
 * Promo codes and gift cards are a later phase (order entity already
 * carries discountCents/giftCardCents from the order slice, so this PR
 * simply always passes 0 for both — the follow-up promotions PR is what
 * wires req.body.promoCode/giftCardCode back through to real redemption).
 */
export const checkout = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Checkout',
    access: 'internal',
    summary:
      'Convert a cart into an order, validating stock first, and initiate payment for its total. provider defaults to stripe.',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: {
      cartId: string,
      shippingAddress: ShippingAddressSchema,
      provider: optional(enum_(PaymentProvider))
    },
    responses: {
      200: CheckoutResultSchema,
      400: string,
      502: string
    }
  },
  async (req, res) => {
    const cart = await cartServiceFactory().getCart({ id: req.body.cartId });

    // Checkout idempotency (retry safety). If this cart already
    // has a still-PENDING order — e.g. an earlier checkout call for it got
    // as far as creating the order but never got as far as (or failed at)
    // creating the payment, see the payment try/catch below — reuse that
    // order rather than creating a second one.
    //
    // Reusing the order is only half of it: the payment-reuse block further
    // down is what stops the retry opening a second live provider payment
    // against that reused order. Neither half is sufficient alone.
    //
    // Scoped to PENDING specifically, not just cartId: a cart can
    // legitimately be reused for a second, later checkout once the order it
    // produced has moved past PENDING (see order.entity.ts's cartId
    // comment) — matching on cartId alone would wrongly "reuse" (and thus
    // block a fresh checkout for) a cart whose prior order already
    // succeeded or was cancelled. OrderCartLookupService's query already
    // encodes this by filtering on status = PENDING, not just cartId.
    const existingPendingOrderId =
      await orderCartLookupServiceFactory().findPendingOrderIdByCartId(cart.id);

    // The empty-cart rejection is deliberately *after* that lookup, not
    // before it. A successful checkout clears the cart (see the clearCart
    // call below), so on any retry this cart is empty — and rejecting empty
    // carts first made the reuse path above unreachable in precisely the
    // situation it exists for. The client got a 400 and, having no order to
    // resume, went on to build a fresh cart and open a second order for the
    // same purchase. Observed live: swapping payment provider at checkout
    // produced two orders for one basket, the abandoned one still holding an
    // open provider payment.
    //
    // An empty cart with no pending order is still a genuine error: there is
    // nothing to buy and nothing to resume.
    if (!cart.items.length && !existingPendingOrderId) {
      res.status(400).send('Cannot checkout an empty cart');
      return;
    }

    let order;
    if (existingPendingOrderId) {
      order = await orderServiceFactory().getOrder({
        id: existingPendingOrderId
      });
      openTelemetryCollector.info(
        'Reusing existing pending order for checkout retry',
        { cartId: cart.id, orderId: order.id }
      );
    } else {
      // Stock check up front, before anything is priced or persisted — a
      // customer should never be told an order succeeded and then find out
      // separately that an item was unavailable.
      const stockChecks = await Promise.all(
        cart.items.map((item) =>
          inventoryServiceFactory().checkStock({
            variantId: item.variantId,
            requested: item.quantity
          })
        )
      );
      const outOfStock = stockChecks.filter((check) => !check.available);
      if (outOfStock.length) {
        res
          .status(400)
          .send(
            `Insufficient stock for variant(s): ${outOfStock
              .map((c) => c.variantId)
              .join(', ')}`
          );
        return;
      }

      // Price each line from the variant's current price — the cart itself
      // only carries variantId + quantity, never a price (prices can move
      // between add-to-cart and checkout; checkout always uses the price at
      // the moment of purchase).
      const variants = await Promise.all(
        cart.items.map((item) =>
          variantServiceFactory().getVariant({ id: item.variantId })
        )
      );
      const orderItems = cart.items.map((item, i) => ({
        variantId: item.variantId,
        quantity: item.quantity,
        unitPriceCents: variants[i].priceCents
      }));

      const subtotalCents = orderItems.reduce(
        (sum, item) => sum + item.quantity * item.unitPriceCents,
        0
      );

      // No promo code in this slice — discount is always 0 until the
      // promotions PR lands (see the module-level comment above).
      const discountCents = 0;
      const discountedSubtotalCents = Math.max(
        0,
        subtotalCents - discountCents
      );

      const [taxResult, shippingResult] = await Promise.all([
        taxServiceFactory().calculate({
          // Taxed on the discounted amount, as one line — not prorated
          // across the original per-item lines, a reasonable v1
          // simplification.
          lineItemCents: [discountedSubtotalCents],
          shippingAddress: req.body.shippingAddress
        }),
        shippingServiceFactory().calculate({
          shippingAddress: req.body.shippingAddress,
          subtotalCents: discountedSubtotalCents
        })
      ]);
      if (taxResult.estimated) {
        openTelemetryCollector.warn(
          'Order tax is a flat estimate — Stripe Tax was unreachable',
          { cartId: cart.id }
        );
      }

      // No gift card in this slice — applied amount is always 0 until the
      // promotions PR lands (see the module-level comment above).
      const giftCardCents = 0;
      const totalCents = Math.max(
        0,
        discountedSubtotalCents +
          taxResult.taxCents +
          shippingResult.shippingCents -
          giftCardCents
      );

      // 3rd arg is the cartId association (see order.mappers.ts's
      // CreateOrderMapper.toEntity) — what makes the reuse lookup above
      // possible on a future retry for this same cart.
      //
      // The existingPendingOrderId lookup above is a plain findOne, not a
      // lock — two concurrent checkout calls for the same cart (a
      // double-clicked "Pay", or a client retry racing the original) can
      // both read no-pending-order and both reach this createOrder call.
      // order.entity.ts's partial unique index on (cartId where status =
      // PENDING) is what actually closes that race: only one of the two
      // inserts can succeed, and the loser gets a
      // UniqueConstraintViolationException here rather than silently
      // creating a second order (and, worse, a second live payment) for
      // the same cart.
      try {
        order = await orderServiceFactory().createOrder(
          {
            customerId: cart.customerId,
            items: orderItems,
            shippingAddress: req.body.shippingAddress,
            subtotalCents,
            discountCents,
            taxCents: taxResult.taxCents,
            taxBreakdown: taxResult.breakdown,
            shippingCents: shippingResult.shippingCents,
            giftCardCents,
            totalCents
          },
          undefined,
          cart.id
        );
      } catch (error) {
        if (!(error instanceof UniqueConstraintViolationException)) {
          throw error;
        }
        // Lost the race — the same insert-first-then-look-up-the-winner
        // pattern WebhookEventService.beginProcessing uses for its own
        // unique-constraint race. The winning request's order is now
        // committed (that's what caused our own insert to fail), so this
        // lookup will find it.
        const raceWinnerOrderId =
          await orderCartLookupServiceFactory().findPendingOrderIdByCartId(
            cart.id
          );
        if (!raceWinnerOrderId) {
          throw error;
        }
        order = await orderServiceFactory().getOrder({
          id: raceWinnerOrderId
        });
        openTelemetryCollector.info(
          'Lost race to a concurrent checkout for this cart — reusing the pending order it created',
          { cartId: cart.id, orderId: order.id }
        );
      }
    }

    // Payment is created against the order's total, routed by provider the
    // same way payment.controller.ts's own createPayment endpoint routes
    // (provider defaults to stripe). Payment.orderId is required, so this
    // can only happen once the order exists — order creation and payment
    // creation can't be one atomic step.
    const usePaypal = req.body.provider === PaymentProvider.PAYPAL;
    const paymentServiceFactory = usePaypal
      ? paypalPaymentServiceFactory
      : stripePaymentServiceFactory;

    // The payment half of the idempotency above. A retry that reused the
    // pending order has to reuse that order's pending payment too, because
    // creating a second one leaves two independently confirmable provider
    // payments for a single order. Both downstream guards still hold in
    // that state — the state machine rejects the second PAID transition and
    // worker.ts never decrements stock twice — so order state and inventory
    // stay correct and the damage lands entirely on the customer: two
    // charges for one order, with nothing to refund the surplus.
    //
    // An order carries at most one live payment, enforced by Payment's
    // payment_order_id_pending_unique partial index. That is deliberately
    // provider-agnostic: which provider created a payment is only
    // recoverable from the shape of its reference (Stripe PaymentIntent ids
    // are prefixed `pi_`; PayPal order ids are not), because Payment has no
    // provider column. So a customer who checks out with Stripe and retries
    // asking for PayPal resumes the Stripe payment rather than opening a
    // PayPal one alongside it — switching providers requires the current
    // attempt to fail first. Two live payments for one order is the worse
    // outcome, and a provider column on Payment is the real fix for the
    // restriction.
    const resumeExistingPayment = async (existing: {
      id: string;
      providerRef: string;
    }) =>
      existing.providerRef.startsWith(STRIPE_PAYMENT_INTENT_PREFIX)
        ? // Stripe needs the intent refetched: client_secret is never
          // persisted (see checkout.schema.ts), so resuming means asking
          // Stripe for it again.
          stripePaymentServiceFactory().resumePayment({ id: existing.id })
        : // PayPal needs nothing reissued — providerRef *is* the PayPal
          // order id, which is all the PayPal JS SDK needs to render its
          // approval buttons, so the stored record already carries
          // everything a first-time checkout would have returned.
          paypalPaymentServiceFactory().getPayment({ id: existing.id });

    const existingPending =
      await paymentOrderLookupServiceFactory().findPendingPaymentByOrderId(
        order.id
      );

    let payment;
    try {
      if (existingPending) {
        payment = await resumeExistingPayment(existingPending);
        openTelemetryCollector.info(
          'Reusing existing pending payment for checkout retry',
          { orderId: order.id, paymentId: existingPending.id }
        );
      } else {
        try {
          payment = await paymentServiceFactory().createPayment({
            orderId: order.id,
            amountCents: order.totalCents,
            currency: CHECKOUT_CURRENCY
          });
        } catch (error) {
          if (!(error instanceof UniqueConstraintViolationException)) {
            throw error;
          }
          // Lost the race to a concurrent checkout for this same order — the
          // lookup above is a plain findOne, not a lock, so two requests can
          // both see no pending payment and both try to insert. The partial
          // index lets exactly one through; this is the loser, and the same
          // insert-first-then-look-up-the-winner recovery the order path uses
          // a few lines up applies here. Without it the double-clicked "Pay"
          // that this index exists to stop would surface as a 502 instead of
          // quietly resuming the payment that did get created.
          const raceWinner =
            await paymentOrderLookupServiceFactory().findPendingPaymentByOrderId(
              order.id
            );
          if (!raceWinner) {
            throw error;
          }
          payment = await resumeExistingPayment(raceWinner);
          openTelemetryCollector.info(
            'Lost race to a concurrent checkout for this order — resuming the payment it created',
            { orderId: order.id, paymentId: raceWinner.id }
          );
        }
      }
    } catch (error) {
      // Deliberately NOT rolled back or cancelled: the order stays PENDING,
      // which is still an honest state — nothing downstream treats PENDING
      // as success. In particular, worker.ts only decrements inventory on
      // the PAID transition, and that transition is now driven exclusively
      // by the payment webhook (see webhook.controller.ts), never by
      // checkout itself — so a PENDING order with no successful payment
      // ties up nothing. The cart is deliberately NOT cleared either (see
      // below) — the same items are still there for the customer (or a
      // "retry checkout" call) to try again with, rather than forcing a
      // manual re-add of everything.
      openTelemetryCollector.error(
        'Payment could not be initiated for order — order left pending, cart left intact',
        { orderId: order.id, cartId: cart.id, error }
      );
      res
        .status(502)
        .send(
          `Order ${order.id} was created but payment could not be initiated. Please retry checkout.`
        );
      return;
    }

    // Cart is only cleared once a payment attempt has actually been
    // initiated with the provider — clearing it any earlier (e.g.
    // immediately after order creation, as this endpoint used to) would
    // strand the customer with an empty cart and a PENDING order they
    // can't easily retry from if payment initiation fails above.
    //
    // Guarded on its own, separate from the payment try/catch above: by
    // this point a Payment row and a live provider PaymentIntent/PayPal
    // order both already exist, so checkout has, in every way that
    // matters, already succeeded — a clearCart failure here must not turn
    // into an unhandled 500. Letting it propagate would (a) tell the
    // client checkout failed when it didn't, and (b) with the cart
    // deliberately left intact for retry (same reasoning as the payment
    // catch above), invite a retry that — thanks to the cartId reuse above
    // — would not create a second order, but *would* create a second live
    // payment for the reused order, since nothing here dedupes Payment the
    // way the reuse lookup dedupes Order. A stale, uncleared cart is a
    // minor, recoverable inconvenience; a duplicate charge is not.
    try {
      await cartServiceFactory().clearCart({ id: cart.id });
    } catch (error) {
      openTelemetryCollector.error(
        'Cart could not be cleared after successful payment initiation — order and payment both succeeded, continuing',
        { orderId: order.id, cartId: cart.id, error }
      );
    }

    openTelemetryCollector.info('Checkout completed', {
      cartId: cart.id,
      orderId: order.id,
      paymentId: payment.id
    });
    // Only StripePaymentService's createPayment returns clientSecret —
    // PayPal's payment DTO never has it (see checkout.schema.ts). The
    // `typeof` check (rather than just `'clientSecret' in payment`) is
    // what actually narrows this to `string | undefined` for the response
    // schema below — `in` alone leaves it typed `unknown` on this union.
    const clientSecret =
      'clientSecret' in payment && typeof payment.clientSecret === 'string'
        ? payment.clientSecret
        : undefined;
    res.status(200).json({
      order,
      payment,
      clientSecret
    });
  }
);
