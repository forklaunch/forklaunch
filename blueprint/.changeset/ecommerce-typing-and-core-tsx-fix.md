---
"@forklaunch/blueprint-ecommerce-stripe": patch
---

Fix a type-only import written as a value import in blueprint-core, and tighten ecommerce provider-seam typing.

`blueprint/core/registrations.ts` re-exported the Express `Request`, `Response`, `NextFunction`, and `ExpressApplicationOptions` **types** as values. Under tsx-on-source (esbuild strips types without type information) that makes the runtime try to resolve bindings that do not exist, crashing anything loaded that way — including the billing module. They are now re-exported with `export type`.

Ecommerce typing: wrap the base mapper-entity types in `ResolvedEntity<>` (matching billing, so a future relation column can't leak `Reference`/`Collection` wrappers into DTOs), replace an unsafe `as { ids: string[] }` cast in the variant list controller with a type-safe object construction, and narrow the `catch` before treating a thrown value as `Error` in the order-event worker.

Provider-seam tightenings: widen the shared payment `CreatePaymentMapper.toEntity` provider arg to `Stripe.PaymentIntent | PaypalOrder` so it's assignable to both provider services by contravariance, removing an `as unknown as` cast at the PayPal DI wiring; validate the PayPal REST client's `fetch` JSON with type guards (throwing on a bad shape, staying fail-closed on webhook verification) and tighten `PaypalOrder.status` to the Orders-v2 literal union; and type the product where-clauses as `FilterQuery<InferEntity<typeof Product>>` instead of `Record<string, unknown>`.
