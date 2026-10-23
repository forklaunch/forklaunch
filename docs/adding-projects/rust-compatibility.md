---
title: Rust and TypeScript Compatibility
category: Guides
description: Source version baselines, contract formats, implementation choices, and snapshot checks for the Rust port.
---

# Rust and TypeScript compatibility baseline

This matrix records the package versions in the source checkout used for the Rust
port, as of 2026-10-12. All native framework and blueprint crates currently declare
`0.1.0`; these are source versions, not a claim that registry releases exist.
Use the CLI and its bundled native source from the same commit. The validation
baseline through `a64455bf4` is detailed in the
[parity inventory](../../plan/RUST-TYPESCRIPT-PARITY-INVENTORY.md).

The rows identify comparison targets, not a promise of complete compatibility.
The linked inventory records passing fixtures and remaining differences per
package. A TypeScript patch/minor/major update requires rerunning the relevant
contract and runtime fixtures; no version range beyond the recorded baseline is
certified. Private generated modules are application fixtures, not installable
public packages.

| TypeScript source | Version | Distribution | Native counterpart (0.1.0 source) |
| --- | --- | --- | --- |
| [`@forklaunch/bunrun`](../../framework/bunrun/package.json) | `1.2.26` | Package source | CLI Cargo/watch and mixed orchestration |
| [`@forklaunch/common`](../../framework/common/package.json) | `1.2.30` | Package source | forklaunch-common |
| [`@forklaunch/core`](../../framework/core/package.json) | `3.0.1` | Package source | forklaunch-core, forklaunch-core-macros, forklaunch-http, forklaunch-auth, forklaunch-contract, forklaunch-mcp |
| [`@forklaunch/e2e-tests`](../../framework/e2e-tests/package.json) | `0.0.3` | Private fixture | Rust wire/runtime suites |
| [`@forklaunch/express`](../../framework/express/package.json) | `1.2.48` | Package source | forklaunch-axum |
| [`@forklaunch/hyper-express`](../../framework/hyper-express/package.json) | `1.2.48` | Package source | forklaunch-axum |
| [`@forklaunch/infrastructure-s3`](../../framework/infrastructure/S3/package.json) | `1.5.1` | Package source | forklaunch-infrastructure-s3 |
| [`@forklaunch/infrastructure-redis`](../../framework/infrastructure/redis/package.json) | `1.4.19` | Package source | forklaunch-infrastructure-redis |
| [`@forklaunch/internal`](../../framework/internal/package.json) | `1.2.33` | Package source | contract macros, CLI embedded templates and Rust tooling |
| [`@forklaunch/testing`](../../framework/testing/package.json) | `1.2.35` | Package source | forklaunch-testing and runtime fixtures |
| [`@forklaunch/universal-sdk`](../../framework/universal-sdk/package.json) | `1.2.29` | Package source | forklaunch-sdk-rust, forklaunch-sdk-typescript, forklaunch-sdk-codegen and CLI SDKs |
| [`@forklaunch/validator`](../../framework/validator/package.json) | `1.2.31` | Package source | forklaunch-validation and schema contracts |
| [`@forklaunch/ws`](../../framework/ws/package.json) | `1.2.45` | Package source | forklaunch-ws, forklaunch-asyncapi |
| [`@forklaunch/blueprint-billing-base`](../../blueprint/billing-base/package.json) | `0.1.6` | Private fixture | forklaunch-billing-stripe (`http`) + Rust CLI |
| [`@forklaunch/blueprint-billing-stripe`](../../blueprint/billing-stripe/package.json) | `0.0.2` | Private fixture | forklaunch-billing-stripe (`http`) + Rust CLI |
| [`@forklaunch/blueprint-cac-base`](../../blueprint/cac-base/package.json) | `0.1.1` | Private fixture | forklaunch-cac-interface/base/axum and Rust init module cac-base |
| [`@forklaunch/blueprint-client-sdk`](../../blueprint/client-sdk/package.json) | `0.1.1` | Private fixture | forklaunch-blueprint-client-sdk and CLI SDK generation |
| [`@forklaunch/blueprint-core`](../../blueprint/core/package.json) | `1.0.1` | Private fixture | forklaunch-blueprint-core and framework crates |
| [`@forklaunch/blueprint-ecommerce-stripe`](../../blueprint/ecommerce-stripe/package.json) | `0.0.2` | Private fixture | forklaunch-ecommerce-stripe (`http`); Rust CLI ecommerce-stripe module |
| [`@forklaunch/blueprint-iam-base`](../../blueprint/iam-base/package.json) | `0.1.6` | Private fixture | Rust init module iam-base |
| [`@forklaunch/blueprint-iam-better-auth`](../../blueprint/iam-better-auth/package.json) | `0.0.1` | Private fixture | TypeScript only; Rust services use its generated Rust SDK |
| [`@forklaunch/implementation-billing-base`](../../blueprint/implementations/billing/base/package.json) | `1.0.41` | Package source | forklaunch-billing-base |
| [`@forklaunch/implementation-billing-stripe`](../../blueprint/implementations/billing/stripe/package.json) | `1.2.11` | Package source | forklaunch-billing-stripe |
| [`@forklaunch/implementation-cac-base`](../../blueprint/implementations/cac/base/package.json) | `0.2.7` | Package source | forklaunch-cac-base |
| [`@forklaunch/implementation-ecommerce-base`](../../blueprint/implementations/ecommerce/base/package.json) | `1.0.13` | Package source | forklaunch-ecommerce-base |
| [`@forklaunch/implementation-ecommerce-paypal`](../../blueprint/implementations/ecommerce/paypal/package.json) | `1.0.8` | Package source | forklaunch-ecommerce-paypal |
| [`@forklaunch/implementation-ecommerce-stripe`](../../blueprint/implementations/ecommerce/stripe/package.json) | `1.0.8` | Package source | forklaunch-ecommerce-stripe |
| [`@forklaunch/implementation-iam-base`](../../blueprint/implementations/iam/base/package.json) | `1.0.40` | Package source | forklaunch-iam-base (HTTP layer generated into the app) |
| [`@forklaunch/implementation-messaging-base`](../../blueprint/implementations/messaging/base/package.json) | `1.1.7` | Package source | Retired; managed gateway email/SMS |
| [`@forklaunch/implementation-messaging-twilio`](../../blueprint/implementations/messaging/twilio/package.json) | `1.1.7` | Package source | Retired; managed gateway email/SMS |
| [`@forklaunch/implementation-worker-bullmq`](../../blueprint/implementations/worker/bullmq/package.json) | `1.0.38` | Package source | None; Rust `--type bullmq` uses the native Redis queue |
| [`@forklaunch/implementation-worker-database`](../../blueprint/implementations/worker/database/package.json) | `1.0.40` | Package source | forklaunch-worker-database |
| [`@forklaunch/implementation-worker-kafka`](../../blueprint/implementations/worker/kafka/package.json) | `1.0.38` | Package source | forklaunch-worker-kafka |
| [`@forklaunch/implementation-worker-redis`](../../blueprint/implementations/worker/redis/package.json) | `1.0.38` | Package source | forklaunch-worker-redis |
| [`@forklaunch/interfaces-billing`](../../blueprint/interfaces/billing/package.json) | `1.0.38` | Package source | forklaunch-billing-interface |
| [`@forklaunch/interfaces-cac`](../../blueprint/interfaces/cac/package.json) | `0.2.6` | Package source | forklaunch-cac-interface |
| [`@forklaunch/interfaces-ecommerce`](../../blueprint/interfaces/ecommerce/package.json) | `1.0.12` | Package source | forklaunch-ecommerce-interface |
| [`@forklaunch/interfaces-iam`](../../blueprint/interfaces/iam/package.json) | `1.0.37` | Package source | forklaunch-iam-interface |
| [`@forklaunch/interfaces-messaging`](../../blueprint/interfaces/messaging/package.json) | `1.1.6` | Package source | Retired; managed gateway email/SMS |
| [`@forklaunch/interfaces-worker`](../../blueprint/interfaces/worker/package.json) | `1.0.35` | Package source | forklaunch-worker-interface |
| [`@forklaunch/blueprint-messaging-base`](../../blueprint/messaging-base/package.json) | `0.1.1` | Private fixture | Retired; managed gateway email/SMS |
| [`@forklaunch/blueprint-messaging-twilio`](../../blueprint/messaging-twilio/package.json) | `0.0.2` | Private fixture | Retired; managed gateway email/SMS |
| [`@forklaunch/blueprint-monitoring`](../../blueprint/monitoring/package.json) | `0.1.1` | Private fixture | forklaunch-core telemetry |
| [`@forklaunch/blueprint-sample-worker`](../../blueprint/sample-worker/package.json) | `0.1.1` | Private fixture | generated durable workers |

## Artifact compatibility

| Surface | Supported baseline | Boundary |
| --- | --- | --- |
| Native HTTP contract | `forklaunch.contract.v1`, generator `framework-rs` | Regenerate with the matching CLI when upgrading; unknown versions fail validation. |
| TypeScript HTTP export | OpenAPI documents, including an explicit empty-key default version bundle | Named-version selection remains open. |
| Portable socket contract | `formatVersion: 1` | JSON/text WebSocket profile; explicit raw binary metadata remains open. |
| External AsyncAPI | 3.0.0 JSON/binary WebSocket server profile; five JSON Schema dialects | Bounded traits and fixed same/separate-channel replies supported; event projects, other protocols/formats, upgrade auth/parameters, dynamic reply addresses and richer matching remain open. |
| Generated TypeScript SDK | HTTP fetch/SSE and Node WebSocket packages | Consumer dependency versions are recorded in generated package.json; live service imports remain open. |
| Generated Rust SDK | Matching bundled framework source, Rust edition 2024 | Registry installation compatibility remains unverified. |

## Upgrading a mixed application

Keep generated framework/blueprint sources together and regenerate contracts and
both SDK languages with the new CLI. Run `forklaunch sdk generate --target all --from all --check` against the application after generation, then compile its workspaces and
exercise HTTP/MCP plus any enabled provider, worker and WebSocket surfaces.
`--check` detects generated-output drift; it does not establish runtime or provider
compatibility. Native Redis/database worker protocols differ from TypeScript
protocols; use the explicit interop fixtures before sharing producers/consumers.

## Implementation choices

OpenAPI uses the ForkLaunch contract registry and its native emitter as the
canonical source. Neither `aide` nor `utoipa` is a required or currently shipped
adapter. This keeps OpenAPI, SDK and MCP metadata on the same registry. An optional
adapter can be introduced later only if it preserves that contract and passes the
same fixtures; maintaining a second independent route inventory is unnecessary.

Coercion is explicit at transport boundaries: path/query string helpers support
numbers, booleans, UUIDs and timestamps. JSON bodies follow their declared serde
and schema types, without general string-to-number/boolean conversion. Booleans
accept `true`, `false`, `1`, `0`; numeric helpers reject empty/whitespace/non-finite
values. Compiled socket codecs additionally normalize safe integral JSON decimal
and exponent spellings for JavaScript-number compatibility. See the
[validation policy](../../framework-rs/crates/validation/src/lib.rs)
and socket conformance fixtures for the exact behavior.

PostgreSQL persistence standardizes on optional `sqlx` integrations, with domain
store traits separating providers from service logic. Tenant transactions,
encryption, audit writes and migrations remain explicit. A SeaORM adapter is not
part of the required parity baseline; an application can implement the store
traits with another backend, but must establish equivalent isolation and audit
behavior. No generic ORM compatibility is implied by the shared traits.

## Cross-language snapshot review

The CI parity suite checks committed JSON baselines for 66 configured billing
routes and 28 DTO schemas, plus 34 TypeScript/native AsyncAPI WebSocket documents.
The tests execute repository TypeScript sources and retain their native comparisons
before checking the snapshots. JSON normalization sorts object keys only; array
order, literal values and schema constraints remain visible in review. Run the
corresponding `framework-rs/parity/rust_billing_contracts_parity.mjs` or
`framework-rs/parity/rust_ws_metadata_parity.mjs` fixture with its pinned test dependencies.
Set `FORKLAUNCH_UPDATE_SNAPSHOTS=1` only when intentionally updating baselines, then
review the diff and rerun without the flag. CI refuses snapshot updates.
