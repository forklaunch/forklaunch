---
'@forklaunch/core': patch
---

- The managed gateway clients (`createModelGatewayClient`, `createSmsClient`, `createWhatsAppClient`, `createVoiceClient`, `createStripeClient`, and the shared `instanceGatewaySettings`) no longer say they are only for managed instances. Their refusal names the managed gateway settings (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`), which the platform sets for a managed instance and, at deploy, for a component of a regular app that declares the capability (`forklaunch infra add <service> <capability>`). Code that matched the old message text ("managed mode", "not a managed instance") should match on the error's settings names instead.
