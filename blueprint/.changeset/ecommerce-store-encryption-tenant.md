---
"@forklaunch/blueprint-ecommerce-stripe": patch
---

Bind a tenant for the store's encrypted columns. No ecommerce entity has a tenant column and the routes are internal HMAC calls, so the EntityManager factory bound no tenant, and on `@forklaunch/core` 3 every order write (`Order.shippingAddress` is PII) failed with `UnboundTenantError`. The factory now binds `STORE_ENCRYPTION_TENANT` (`'ecommerce:store'`) when the DI context carries no tenant. Orders written by core 2.x were sealed under the empty tenant, which core 3 no longer accepts; re-encrypt them under `'ecommerce:store'` before reading them.
