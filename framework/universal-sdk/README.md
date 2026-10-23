
### Native service declarations

A native service publishes type-only `sdk.ts` declarations, including its named
DTOs and `NotesSdkClient`. Consumers import those types and use this package:

```ts
import type { NotesSdkClient, HealthResponse } from '@app/notes/sdk';
import { universalSdk } from '@forklaunch/universal-sdk';

const notesSdkClient = universalSdk<NotesSdkClient>;
const notes = await notesSdkClient({
  host: 'https://notes.example.test',
  registryOptions: { url: 'https://notes.example.test/api-definitions' }
});
```

Native registry operations carry `x-forklaunch-operation` descriptors. The
framework validates their format and dispatches through the checked portable
engine, retaining schema, status, media, header, date and SSE validation. Unknown
formats fail closed. A type import alone cannot supply runtime validation;
consumers must use the matching checked registry and this runtime version.
Ordinary TypeScript OpenAPI registries keep the existing dispatch path.

`src/portableRuntime.ts` is a framework-owned generated snapshot of the canonical
Rust compiler runtime. The core `sdk_package_layout` test checks exact equality;
regenerate from its `FORKLAUNCH_PACKAGE_SDK_FIXTURE` output when changing that
runtime. Service packages contain declarations, not copies of this engine.

### Service-to-service HMAC

Internal (`access: 'internal'`) routes take the ForkLaunch request HMAC. Pass the
target service's key and the SDK signs every request itself, with a fresh timestamp
and random nonce, over the final method, URL path (no query) and body bytes:

```ts
const billing = await universalSdk<BillingSdkClient>({
  host: process.env.BILLING_URL!,
  registryOptions: { url: `${process.env.BILLING_URL}/openapi.json` },
  hmac: { secretKey: process.env.BILLING_HMAC_SECRET_KEY! } // keyId defaults to 'default'
});
```

`createHmacAuthorization` and `hmacSignedFetch` expose the same signer for
hand-written calls. The signed path is the full wire path (`POST /payment-link`),
which every verifier accepts. Express routers used to verify the router-relative
`req.path` (`/`); that form is still accepted for existing callers but is
deprecated, because it is not bound to a mount. Servers can reject it with
`HMAC_ACCEPT_MOUNT_RELATIVE_PATHS=false`.
