import { describe, expect, it } from 'vitest';
import { StripePaymentService } from '../services/payment.service';

// A checkout retry must resume the PaymentIntent the order already has
// rather than opening a second one. Two live intents for one order are both
// confirmable, and a customer who confirms both is charged twice — the order
// state machine and the inventory decrement stay correct, so nothing else in
// the system reveals the problem.

type Call = { id: string; options: unknown };

/**
 * Builds the service with a Stripe double that records what it was asked to
 * do, so a test can assert an intent was NOT created as well as that one was
 * retrieved.
 */
const build = (
  payment: Record<string, unknown>,
  clientSecret: string | null,
  connect?: { connectedAccountId: string; platformFeeBps?: number }
) => {
  const retrieved: Call[] = [];
  const created: unknown[] = [];
  const service = new StripePaymentService(
    {
      paymentIntents: {
        retrieve: async (id: string, _params: unknown, options: unknown) => {
          retrieved.push({ id, options });
          return { id, client_secret: clientSecret };
        },
        create: async (params: unknown) => {
          created.push(params);
          return { id: 'pi_newly_created', client_secret: 'secret_new' };
        }
      }
    } as never,
    {} as never,
    { warn: () => undefined, error: () => undefined } as never,
    {} as never,
    {} as never,
    connect ? { connect } : undefined
  );
  service.basePaymentService = {
    getPayment: async () => payment
  } as never;
  return { service, retrieved, created };
};

describe('StripePaymentService.resumePayment', () => {
  it('reissues the client secret from the existing intent', async () => {
    const { service, retrieved, created } = build(
      { id: 'pay_1', orderId: 'ord_1', providerRef: 'pi_existing' },
      'pi_existing_secret_abc'
    );

    const result = await service.resumePayment({ id: 'pay_1' });

    expect(result.clientSecret).toBe('pi_existing_secret_abc');
    expect(retrieved.map((call) => call.id)).toEqual(['pi_existing']);
    // The whole point: no second intent for this order.
    expect(created).toEqual([]);
  });

  it('preserves the stored payment record alongside the secret', async () => {
    const { service } = build(
      {
        id: 'pay_1',
        orderId: 'ord_1',
        amountCents: 5350,
        status: 'pending',
        providerRef: 'pi_existing'
      },
      'secret'
    );

    const result = await service.resumePayment({ id: 'pay_1' });

    expect(result).toMatchObject({
      id: 'pay_1',
      orderId: 'ord_1',
      amountCents: 5350,
      status: 'pending',
      providerRef: 'pi_existing'
    });
  });

  it('returns the record untouched when the payment never reached Stripe', async () => {
    // No providerRef means no intent exists to resume. Callers treat this
    // the same as finding no payment at all, and create a fresh one.
    const { service, retrieved } = build(
      { id: 'pay_1', orderId: 'ord_1', providerRef: undefined },
      'unused'
    );

    const result = await service.resumePayment({ id: 'pay_1' });

    expect(result.clientSecret).toBeUndefined();
    expect(retrieved).toEqual([]);
  });

  it('omits clientSecret when Stripe returns an intent without one', async () => {
    const { service } = build(
      { id: 'pay_1', orderId: 'ord_1', providerRef: 'pi_existing' },
      null
    );

    const result = await service.resumePayment({ id: 'pay_1' });

    expect(result.clientSecret).toBeUndefined();
  });

  it('retrieves against the connected account in Connect mode', async () => {
    // A direct charge lives on the merchant's account, so the retrieve has
    // to be scoped there too — a platform-scoped lookup would not find it.
    const { service, retrieved } = build(
      { id: 'pay_1', orderId: 'ord_1', providerRef: 'pi_existing' },
      'secret',
      { connectedAccountId: 'acct_merchant', platformFeeBps: 0 }
    );

    await service.resumePayment({ id: 'pay_1' });

    expect(retrieved[0].options).toEqual({ stripeAccount: 'acct_merchant' });
  });

  it('retrieves without an account scope in bring-your-own-key mode', async () => {
    const { service, retrieved } = build(
      { id: 'pay_1', orderId: 'ord_1', providerRef: 'pi_existing' },
      'secret'
    );

    await service.resumePayment({ id: 'pay_1' });

    expect(retrieved[0].options).toBeUndefined();
  });
});
