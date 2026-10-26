---
"@forklaunch/blueprint-ecommerce-stripe": patch
---

The ecommerce module routes `createInventory` (`POST /inventory`, internal HMAC) and exposes it on the SDK as `inventory.createInventory`, so a variant's stock record can be created over HTTP. Webhook routes read the framework's raw body (`req._rawBody`) instead of an Express-only `json.verify` hook, so Stripe and PayPal signature checks work on hyper-express too. Shipped to new apps through the CLI templates; this package is not published.
