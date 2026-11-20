---
'@forklaunch/implementation-billing-base': major
'@forklaunch/implementation-billing-stripe': major
'@forklaunch/implementation-cac-base': minor
'@forklaunch/implementation-ecommerce-base': major
'@forklaunch/implementation-ecommerce-paypal': major
'@forklaunch/implementation-ecommerce-stripe': major
'@forklaunch/implementation-iam-base': major
'@forklaunch/implementation-messaging-base': major
'@forklaunch/implementation-messaging-twilio': major
'@forklaunch/implementation-worker-bullmq': major
'@forklaunch/implementation-worker-database': major
'@forklaunch/implementation-worker-kafka': major
'@forklaunch/implementation-worker-redis': major
'@forklaunch/interfaces-billing': major
'@forklaunch/interfaces-cac': minor
'@forklaunch/interfaces-ecommerce': major
'@forklaunch/interfaces-iam': major
'@forklaunch/interfaces-messaging': major
---

Built on `@forklaunch/core` 4 and `@mikro-orm/core` 7.2.10. The previous versions depend on core 3 and pin MikroORM 7.2.5, so an application on core 4 that used them installed two copies of each and its entity types stopped matching.

Breaking, because core 4 changes defaults these packages run with: cross-origin browser access is denied unless the application names its origins (`CORS_ORIGINS` or a `cors` option), an HMAC-signed request is accepted once and only within 5 minutes of the server clock (multi-instance services need a shared nonce store), and JWT verification pins the signature algorithm. See the `@forklaunch/core` 4.0.0 changelog for what to change. 0.x packages take a minor bump, the 0.x equivalent of a major.
