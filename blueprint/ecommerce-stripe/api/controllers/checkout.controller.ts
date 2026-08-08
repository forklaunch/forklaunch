import { enum_, handlers, optional, schemaValidator, string } from '../../schema';
import {
  CheckoutResultSchema,
  ShippingAddressSchema
} from '../../domain/schemas/checkout.schema';
import { PaymentProvider } from '../../domain/enum/paymentProvider.enum';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const cartServiceFactory = ci.scopedResolver(tokens.CartService);
const variantServiceFactory = ci.scopedResolver(tokens.VariantService);
const inventoryServiceFactory = ci.scopedResolver(tokens.InventoryService);
const orderServiceFactory = ci.scopedResolver(tokens.OrderService);
const orderCartLookupServiceFactory = ci.scopedResolver(
  tokens.OrderCartLookupService
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
 * The unified checkout orchestration (ECOM-09/10): cart -> order -> payment
 * in one call, with stock validated up front so a customer never pays for
 * something that's not actually there. Tax (Stripe Tax) and shipping
 * (flat-rate) are both real, not stubs — a shipping address is required
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

    if (!cart.items.length) {
      res.status(400).send('Cannot checkout an empty cart');
      return;
    }

    // Checkout idempotency (ECOM-09/10 retry safety). If this cart already
    // has a still-PENDING order — e.g. an earlier checkout call for it got
    // as far as creating the order but never got as far as (or failed at)
    // creating the payment, see the payment try/catch below — reuse that
    // order rather than creating a second one, so a retry doesn't also end
    // up creating a second live payment for the same cart.
    //
    // Scoped to PENDING specifically, not just cartId: a cart can
    // legitimately be reused for a second, later checkout once the order it
    // produced has moved past PENDING (see order.entity.ts's cartId
    // comment) — matching on cartId alone would wrongly "reuse" (and thus
    // block a fresh checkout for) a cart whose prior order already
    // succeeded or was cancelled. OrderCartLookupService's query already
    // encodes this by filtering on status = PENDING, not just cartId.
    const existingPendingOrderId =
      await orderCartLookupServiceFactory().findPendingOrderIdByCartId(
        cart.id
      );

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
    }

    // Payment is created against the order's total, routed by provider the
    // same way payment.controller.ts's own createPayment endpoint routes
    // (provider defaults to stripe). Payment.orderId is required, so this
    // can only happen once the order exists — order creation and payment
    // creation can't be one atomic step.
    const paymentServiceFactory =
      req.body.provider === PaymentProvider.PAYPAL
        ? paypalPaymentServiceFactory
        : stripePaymentServiceFactory;

    let payment;
    try {
      payment = await paymentServiceFactory().createPayment({
        orderId: order.id,
        amountCents: order.totalCents,
        currency: CHECKOUT_CURRENCY
      });
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
