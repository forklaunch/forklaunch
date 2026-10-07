import { describe, expect, it } from 'vitest';
import { StripePaymentService } from '../services/payment.service';

// Stripe requires application_fee_amount to be strictly less than the charge.
// The fee comes from STRIPE_PLATFORM_FEE_BPS, so a bad value is an operator
// mistake that would otherwise surface as a rejected PaymentIntent at a
// customer's checkout rather than at startup.
const build = (platformFeeBps?: number) =>
  new StripePaymentService(
    {} as never,
    {} as never,
    { warn: () => undefined } as never,
    {} as never,
    {} as never,
    {
      connect: { connectedAccountId: 'acct_test', platformFeeBps }
    }
  );

describe('Stripe Connect platform fee validation', () => {
  it('accepts the launch default of no fee', () => {
    expect(() => build(0)).not.toThrow();
    expect(() => build(undefined)).not.toThrow();
  });

  it('accepts a fee below the whole charge', () => {
    expect(() => build(250)).not.toThrow();
    expect(() => build(9999)).not.toThrow();
  });

  it('rejects 10000 bps, which would make the fee equal the charge', () => {
    expect(() => build(10000)).toThrow(/0 to 9999/);
  });

  it('rejects a fee above the charge', () => {
    expect(() => build(10001)).toThrow(/0 to 9999/);
  });

  it('rejects negative and non-integer values', () => {
    expect(() => build(-1)).toThrow(/0 to 9999/);
    expect(() => build(12.5)).toThrow(/0 to 9999/);
    expect(() => build(Number.NaN)).toThrow(/0 to 9999/);
  });
});
