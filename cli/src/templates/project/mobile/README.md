# Mobile app

Run `pnpm install`, set `EXPO_PUBLIC_IAM_URL` to your public IAM service, and use `pnpm ios` or `pnpm android` to create a development build. Then `pnpm dev` starts the development server. iOS builds need macOS and Xcode, or a configured remote build service; Android builds need the Android toolchain. A browser preview is not a native test.

The IAM service must enable Better Auth’s Expo plugin and trust only this app’s generated scheme. Configure account registration and recovery on the same service. Sessions use Expo SecureStore; provider keys belong only on the backend. Do not add arbitrary return URLs or broad production origin wildcards.

Publishing to app stores, signing, device permissions, and build-service billing are separate setup steps. This starter requests no camera, contacts, or notification access.
