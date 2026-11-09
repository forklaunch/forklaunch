# Web and mobile apps

Create a frontend inside an existing ForkLaunch application:

```sh
forklaunch init frontend --path ./my-app --name 'My App' --target web
forklaunch init frontend --path ./my-app --name 'My App' --target mobile
forklaunch init frontend --path ./my-app --name 'My App' --target both --dryrun
```

Web creates `apps/web` with React, Vite, navigation, and Better Auth sign-in. Mobile creates `apps/mobile` with Expo SDK 58, Expo Router, development-build commands, and protected session storage. Add `--assistant` to include an assistant screen with activity and explicit action confirmation, connected through the public `VITE_AGENT_URL` or `EXPO_PUBLIC_AGENT_URL`. Both clients use the same backend. A mobile frontend does not add a second IAM service or expose model credentials.

The command performs no network installation, Git commit, or backend mutation. Run `pnpm install` in each generated app and review its README for connection setup. Existing destinations, legacy `apps/client`, and redirected `apps` folders are rejected before writing. Each generated package records the CLI version, starter version, target, and Expo SDK where applicable. Updating a starter never overwrites existing product code.

Set the public IAM address in the frontend environment. Web sessions use server cookies. Mobile requires Better Auth's Expo server plugin and the generated app scheme in trusted origins; device sessions use SecureStore. Keep trusted return links limited to this app, and keep backend/provider secrets on the server. The CLI does not silently change your authentication policy.

Run the web build and a real mobile development build before shipping. Mobile sign-in, session restore and product actions need checks on iOS and Android. A web preview is not native-device verification. Native signing, remote-build accounts and app-store submission are separate from backend deployment.
