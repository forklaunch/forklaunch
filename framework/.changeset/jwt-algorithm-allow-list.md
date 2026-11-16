---
'@forklaunch/core': major
---

BREAKING: JWT verification always pins the signature algorithm. `jwt: { signatureKey }` accepts `HS256` only; `jwt: { jwksPublicKey }` and `jwt: { jwksPublicKeyUrl }` accept only the asymmetric algorithms that fit each key (RS*/PS* for RSA, ES256/384/512 by curve, EdDSA for Ed25519, or the key's own `alg`), never an HMAC algorithm and never `none`. Who is affected: services whose tokens are signed with `HS384` or `HS512`, and services that passed a symmetric (`kty: 'oct'`) JWK as `jwksPublicKey`. Fix: add the algorithm to the auth config, for example `jwt: { signatureKey, algorithms: ['HS512'] }`, and move a shared secret to `signatureKey`. New optional settings on every `jwt` config, also accepted by `bindRuntimeAuth`: `algorithms` (narrows the list), `issuer` and `audience` (checked only when set; an empty value rejects tokens instead of switching the check off).
