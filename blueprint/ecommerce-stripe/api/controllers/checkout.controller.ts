import { handlers, schemaValidator, string } from '../../schema';
import { ShippingAddressSchema } from '../../domain/schemas/checkout.schema';
import { OrderMapper } from '../../domain/mappers/order.mappers';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const cartServiceFactory = ci.scopedResolver(tokens.CartService);
const variantServiceFactory = ci.scopedResolver(tokens.VariantService);
const inventoryServiceFactory = ci.scopedResolver(tokens.InventoryService);
const orderServiceFactory = ci.scopedResolver(tokens.OrderService);
const taxServiceFactory = ci.scopedResolver(tokens.TaxService);
const shippingServiceFactory = ci.scopedResolver(tokens.ShippingService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

/**
 * The unified checkout orchestration (ECOM-09): cart -> order in one call,
 * with stock validated up front so a customer never pays for something
 * that's not actually there. Tax (Stripe Tax) and shipping (flat-rate) are
 * both real, not stubs — a shipping address is required because both need
 * one.
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
    summary: 'Convert a cart into an order, validating stock first',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: {
      cartId: string,
      shippingAddress: ShippingAddressSchema
    },
    responses: {
      200: OrderMapper.schema,
      400: string
    }
  },
  async (req, res) => {
    const cart = await cartServiceFactory().getCart({ id: req.body.cartId });

    if (!cart.items.length) {
      res.status(400).send('Cannot checkout an empty cart');
      return;
    }

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
    const discountedSubtotalCents = Math.max(0, subtotalCents - discountCents);

    const [taxResult, shippingResult] = await Promise.all([
      taxServiceFactory().calculate({
        // Taxed on the discounted amount, as one line — not prorated across
        // the original per-item lines, a reasonable v1 simplification.
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

    const order = await orderServiceFactory().createOrder({
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
    });

    await cartServiceFactory().clearCart({ id: cart.id });

    openTelemetryCollector.info('Checkout completed', {
      cartId: cart.id,
      orderId: order.id
    });
    res.status(200).json(order);
  }
);
