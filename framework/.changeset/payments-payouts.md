---
'@forklaunch/core': patch
---

Payments: `createPaymentsClient()` gains `payouts({ limit })` (the connected account's balance, recent payouts, bank and payout schedule; read-only, empty before onboarding) and `dashboardLink()` (Stripe's own page for changing the bank and getting tax forms; 409 before onboarding), with the `PaymentsPayouts`, `PaymentsPayout` and `PaymentsMoney` types. The gateway mock serves both routes.
