# Local agent model access

A local agent can use the user's existing ForkLaunch login. It does not need an Azure/OpenAI key and must never read the account token file itself.

After this CLI/framework version is installed:

```sh
forklaunch login
# Run your agent from its integrated ForkLaunch project directory.
```

The CLI discovers the application ID from the current project manifest. Outside the project directory, set `FORKLAUNCH_APPLICATION_ID` explicitly. It never creates or selects a different app silently.

In server-side agent code:

```ts
import { createLocalModelGatewayClient } from '@forklaunch/core/http';
const models = createLocalModelGatewayClient();
const catalog = await models.models();
const result = await models.chat.completions.create({
  model: catalog.models[0].id,
  messages: [{ role: 'user', content: 'Explain this synthetic example.' }]
});
```

Resolution order:
1. Explicit `FORKLAUNCH_API_KEY`: the CLI exchanges it without saving it. A refused key does not fall back to another account.
2. A desktop-provided relay (`FORKLAUNCH_LOCAL_MODEL_RELAY=1`), with its injected local connection settings.
3. The CLI's selected account (`--account` / `FORKLAUNCH_ACCOUNT`), or default CLI login.

The CLI helper is `forklaunch model credential --application-id <UUID> --json`. Its output is a **secret limited credential**, intended for a trusted process pipe. It refuses to display credentials on a terminal. The account token is never emitted. Issuance requires access to the saved app and the server's local-runtime model policy; service-account keys must have the necessary app/organization access.

The local client keeps the credential in memory and renews it after expiry on the next operation. It does not retry inference or replace credentials on a quota refusal. The server remains responsible for authorization and shared allowance enforcement. An injected desktop relay manages its own lifetime separately.

For Docker, the trusted launcher supplies a local relay or scoped credentials through a credential provider. Do not mount `~/.forklaunch` into generated containers, put keys in browser code, or commit them to Git. The default CLI discovery is for trusted local server processes, not untrusted generated code.

Streaming uses `client.chat.completions.stream(request, { signal })`. It requires the matching runtime gateway update. Streaming initially supports text answers; tool requests remain on the non-streaming protocol. Streams must complete explicitly; an interrupted answer is not a successful task. No model reasoning is exposed.

This does not replace the managed-instance client: hosted apps continue to use `createModelGatewayClient` with their instance credentials. The preview catalog makes no managed monthly-budget or compliance claims.
