---
name: expo-mobile
description: Build an Expo mobile frontend for a ForkLaunch app using the CLI-owned foundation.
---

Use `forklaunch init frontend --path <application-root> --name '<display name>' --target mobile` for a new frontend, or `--target both` when web is also required. Add `--assistant` only when the product needs an assistant. Keep existing frontends and edits; never regenerate over them.

The starter uses Expo SDK 58 and Expo Router under `apps/mobile`. Install its packages and use a development build on iOS and Android. Follow Expo's SDK-specific compatibility recommendations; do not update individual native packages independently. Record actual checks and distinguish type checking, bundling, simulator testing and real-device testing.

Connect the public IAM address with `EXPO_PUBLIC_IAM_URL`. Enable the Better Auth Expo server plugin and allow only the generated app scheme in trusted origins. SecureStore holds native sessions. Validate return links against the fixed scheme and known app routes; do not accept arbitrary URLs. Backend and model-provider secrets stay on the backend. Request device permissions only for product features that need them.

With `--assistant`, set the public agent service address in `EXPO_PUBLIC_AGENT_URL`. Use the existing authorized service/tool catalog. The agent backend validates arguments and permissions and seals consequential-action approvals; the UI shows activity and asks the user before approving a mutation. Never treat a client-side confirmation as permission to bypass server checks.

Use platform-appropriate controls and reuse the same backend rules as the web app. Mobile signing, remote builds and store publication require separate setup; neither a browser preview nor a successful backend release proves mobile readiness.
