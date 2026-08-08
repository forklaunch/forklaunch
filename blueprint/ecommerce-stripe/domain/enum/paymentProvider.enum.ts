/**
 * Payment provider routing choice — not a persisted field (Payment has no
 * "provider" column, only providerRef, the chosen provider's own id).
 * Shared between payment.controller.ts (explicit provider selection on
 * `POST /payment`) and checkout.controller.ts (which now creates the
 * payment for an order's total as part of checkout, so it needs the exact
 * same routing choice) — pulled out to one place so the two can't drift.
 */
export const PaymentProvider = { STRIPE: 'stripe', PAYPAL: 'paypal' } as const;

export type PaymentProviderType =
  (typeof PaymentProvider)[keyof typeof PaymentProvider];
