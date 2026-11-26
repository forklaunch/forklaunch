# Literal JSON keys at runtime boundaries

The checked native workspaces use a pinned `serde_json` 1.0.151 source patch.
With upstream `raw_value` or `arbitrary_precision` enabled, the first object key
can be classified as Serde's private RawValue/Number marker. Thus an ordinary
JSON object such as `{"$serde_json::private::Number":"123"}` can become a number;
other values can produce a parse error. This affects nested `serde_json::Value`
fields as well as top-level values. A TypeScript `JSON.parse` object retains the
literal key.

The patch distinguishes real map keys from the internal marker protocol using
the key deserializer's unit-enum path. The internal number/raw key deserializers
forward that request to their existing string visitor, preserving exact numbers
and actual RawValue behavior. The source, upstream tests and both licenses are
bundled under `framework-rs/crates/vendor/serde_json`. The implementation change
is limited to `src/value/de.rs`; this is not a replacement JSON parser.

| Boundary | Before | Patched behavior / evidence |
| --- | --- | --- |
| Generic `forklaunch_validation::parse_json<T>` | `Value` and nested DTO values could interpret marker keys. | Literal objects survive both typed and untyped request parsing. |
| Axum `Json<Value>` | Framework-external extraction inherited unified Serde features. | Real HTTP extractor roundtrips preserve keys; the root patch applies transitively to Axum. |
| rmcp transport, routed MCP bodies and structured responses | Parsing could change arguments before route validation/dispatch, or change the structured response. | End-to-end MCP tool requests pass through rmcp and Axum and retain both text and structured response objects. |
| Typed/manual tools and `from_value` | Owned and borrowed Value deserialization could interpret first-key markers. | Manual tool execution and upstream boundary regressions check typed records and both owned/borrowed Value conversions. |
| `JsonRecordCodec`, Redis JSON stores/queues and S3 JSON stores | Generic record decoding inherited the same Value ambiguity. | Both plaintext and tenant-encrypted records retain root and nested DTO objects. Cache/object adapters share this codec. |
| Blueprint database/auth/provider/worker parsing | Unified dependency features could activate the ambiguity outside auth's raw parser. | Blueprint root uses the same patch. Existing auth's explicit raw parser remains in place for compatibility with standalone consumers. |

`framework-rs`, `blueprint-rs` and newly generated application Cargo roots select
the vendored parser. The framework JSON dependencies pin the matching version so
a newer registry release cannot silently replace the workspace patch. Existing
application generation/sync adds a missing root patch idempotently. Explicit
user patches are preserved and remain the application's responsibility.

A standalone consumer outside these Cargo workspace roots must apply the same
`[patch.crates-io]` entry pointing at the bundled `serde_json` source. A Cargo
patch in a dependency's manifest is not inherited by its consumer. Registry
publication/upstream adoption remains part of the release gate; this audit does
not claim that upstream registry serde_json has adopted the fix.

## Remaining JSON conformance limits

Unpaired UTF-16 surrogates accepted by JavaScript JSON parsing remain rejected by
Rust strings. Default recursion limits remain bounded; the patch does not enable
unbounded-depth parsing. JavaScript numeric rounding/property ordering and ORM
metadata revival are separate compatibility decisions. Arbitrary non-JSON Serde
formats are outside this patch's tested interface. Applications directly using
an unpatched parser outside the checked workspaces retain upstream behavior.

## Reproducible checks

- Vendored parser tests with default and all features, plus alloc-only compilation.
- `cargo test --manifest-path framework-rs/Cargo.toml --workspace --all-features --lib`.
- `cargo test --manifest-path blueprint-rs/Cargo.toml --workspace --all-features --lib`.
- `cli/tests/rust_http_runtime.sh`: generated, relocated Rust service and actual HTTP/MCP marker-key roundtrips beside both language SDKs.
- `cli/tests/rust_packaged_install.sh`: packaged parser sources, root patch and relocated generation.

Validation receipt (2026-10-12): 237 default and 262 all-feature parser tests,
alloc-only build, 242 framework and 108 blueprint library tests, strict framework
Clippy, typed/routed MCP, actual Redis/S3 encrypted wire interoperability and
13,908 actual TS/native WebSocket cases passed. The 859-entry source package
installed and generated portable modules/SDKs with live HTTP/MCP startup; the
separate HTTP fixture verified literal keys over both HTTP and MCP. External
infrastructure tests requiring configured services remain explicitly ignored in
the library-only runs; this receipt does not turn those into successful runs.
