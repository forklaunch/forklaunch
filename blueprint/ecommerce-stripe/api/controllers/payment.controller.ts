import {
  enum_,
  handlers,
  IdSchema,
  number,
  optional,
  schemaValidator,
  string
} from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import { PaymentMapper } from '../../domain/mappers/payment.mappers';

const stripeServiceFactory = ci.scopedResolver(tokens.PaymentService);
const paypalServiceFactory = ci.scopedResolver(tokens.PaypalPaymentService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const PaymentProvider = { STRIPE: 'stripe', PAYPAL: 'paypal' } as const;

/** Provider is a routing choice, not a persisted field — Payment has no
 *  "provider" column, only providerRef (the chosen provider's own id). */
export const createPayment = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Payment',
    access: 'internal',
    summary:
      'Create a payment for an order (drives it toward paid). provider defaults to stripe.',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: {
      orderId: string,
      amountCents: number,
      currency: string,
      provider: optional(enum_(PaymentProvider))
    },
    responses: { 200: PaymentMapper.schema }
  },
  async (req, res) => {
    const { provider, ...paymentDto } = req.body;
    const serviceFactory =
      provider === PaymentProvider.PAYPAL
        ? paypalServiceFactory
        : stripeServiceFactory;
    res.status(200).json(await serviceFactory().createPayment(paymentDto));
  }
);

export const getPayment = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Payment',
    access: 'internal',
    summary: 'Get a payment',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: PaymentMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await stripeServiceFactory().getPayment(req.params));
  }
);
