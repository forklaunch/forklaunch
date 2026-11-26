# Canonical IAM service

This fixture bundles the repository's actual TypeScript Express, core, common and
Zod validator sources; external runtime dependencies are pinned in `package.json`.
There are no replacement handlers, auth middleware or validator implementations.
The native counterpart is `forklaunch-iam-base`'s `examples/canonical_iam.rs`, which
serves the published IAM contracts from the module's `InMemoryIamService`.
Both seed Ada and Grace and use the same test-only HS256 key. The native example
issues a token through `issue_jwt_hs256`; JOSE verifies every claim and both
services accept it, in addition to the JOSE-issued token cases.

```sh
mkdir -p /tmp/fl-canonical-http-dependencies
cp framework-rs/parity/fixtures/canonical-iam/package.json /tmp/fl-canonical-http-dependencies/
npm install --prefix /tmp/fl-canonical-http-dependencies --ignore-scripts
FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES=/tmp/fl-canonical-http-dependencies \
  CARGO_INCREMENTAL=0 node framework-rs/parity/rust_canonical_http_parity.mjs
```

The runner creates an isolated source bundle, starts both services on ephemeral
loopback ports, checks successful reads/lists, missing users and valid/missing/
malformed/expired/forged JWTs and optional NumericDate claim types/boundaries, and retrieves both generated OpenAPI documents.
Telemetry export is disabled for this local fixture. It terminates both processes
and removes its owned temporary source bundle even on failure.

The sample list handler intentionally ignores pagination (matching the existing
Rust sample); this fixture does not claim pagination conformance. Rejected auth
checks assert matching statuses: missing credentials return 401; malformed,
expired and forged JWTs return 403 in both frameworks. Native opaque-session
rejections keep their existing 401 status; this change applies to JWT contracts. Both services now use `authenticated` for JWT-only contracts. Framework error
envelopes and broader access-level validation are separate auth/error parity work.
The runner also exercises the actual TypeScript OpenAPI emitter with default,
explicit and custom JWT headers plus Basic auth: security references must resolve,
non-OAuth scope lists must be empty and path parameters must be required.
Full OpenAPI equivalence, multipart, streaming, other auth modes,
MCP conformance and production IAM persistence remain separate checklist items.

## OpenAPI snapshots

The paired runner checks committed key-sorted full documents, then compares
normalized operation sets, parameters, security requirements and successful
response schemas. Normalization resolves local schema references, sorts sets,
removes schema annotations and equates unconstrained disjoint primitive unions
with type arrays. Validation constraints and response contracts remain visible.
Ten differing error-response entries are separately pinned in
`openapi-differences.json`: TypeScript's implicit error responses and its null
schema for the explicit empty 404 differ from native documents. Snapshot coverage
does not establish full error/document parity. The TypeScript fixture accepts
optional nullable DTO fields and integer limits, matching the native contracts.

To intentionally update snapshots, run the same command with
`UPDATE_CANONICAL_OPENAPI=1`, review all three JSON files, then run without that
variable to verify. `FORKLAUNCH_TEST_OPENAPI_OUTPUT` optionally writes raw documents
to a chosen directory for inspection.
