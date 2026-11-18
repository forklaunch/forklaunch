import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  createPaymentsClient,
  PaymentsRequestError
} from '../../src/http/paymentsClient';
import { type Harness, startHarness } from './harness';

/**
 * Where the money is, end to end against the gateway mock: payouts() and
 * dashboardLink() before and after onboarding, with the payouts limit carried
 * in the signed path.
 */
let h: Harness;
beforeAll(async () => {
  h = await startHarness({});
});
afterAll(async () => {
  await h?.stop();
});

describe('payouts and the dashboard link (e2e)', () => {
  it('answers empty, and refuses a dashboard link, before onboarding', async () => {
    const payments = createPaymentsClient(h.clientOptions);
    expect(await payments.payouts()).toEqual({
      available: [],
      pending: [],
      payouts: [],
      bank: null,
      schedule: null
    });
    const refused = await payments.dashboardLink().catch((e: unknown) => e);
    expect(refused).toBeInstanceOf(PaymentsRequestError);
    expect(refused).toMatchObject({ status: 409 });
  });

  it('shows the balance, payouts, bank and schedule once onboarded', async () => {
    const payments = createPaymentsClient(h.clientOptions);
    await payments.onboardingLink({
      returnUrl: 'https://studio.example.com/settings?done=1',
      refreshUrl: 'https://studio.example.com/settings'
    });
    const view = await payments.payouts({ limit: 5 });
    expect(view.available[0]).toEqual({ amount: 12_500, currency: 'usd' });
    expect(view.payouts).toHaveLength(1);
    expect(view.payouts[0]).toMatchObject({ status: 'paid', automatic: true });
    expect(Number.isNaN(Date.parse(view.payouts[0].arrivalDate))).toBe(false);
    expect(view.bank).toEqual({ bankName: 'STRIPE TEST BANK', last4: '6789' });
    expect(view.schedule).toEqual({ interval: 'daily', delayDays: 2 });
    const { url } = await payments.dashboardLink();
    expect(url).toMatch(/^https:\/\/connect\.stripe\.com\/express\/acct_mock_/);
  });

  it('checks the limit before calling the gateway', () => {
    const payments = createPaymentsClient(h.clientOptions);
    expect(() => payments.payouts({ limit: 0 })).toThrow(TypeError);
    expect(() => payments.payouts({ limit: 51 })).toThrow(TypeError);
    expect(() => payments.payouts({ limit: 2.5 })).toThrow(TypeError);
  });
});
