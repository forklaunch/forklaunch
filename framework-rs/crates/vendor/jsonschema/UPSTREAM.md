# Upstream provenance

Vendored from `jsonschema 0.58.6`, MIT licensed, published source archive
`https://crates.io/api/v1/crates/jsonschema/0.58.6/download`.

Archive SHA-256: `f622c574a29fb3034a0a90645afee194dfedc75fc124228801b6e77041075bd1`.
Upstream Git commit: `55ac1664384793c5d3b20745d3f9c169dfe2c06f`.
Repository: https://github.com/Stranger6667/jsonschema

The package is renamed to `forklaunch-jsonschema-protocol` to scope compatibility
to ForkLaunch socket codecs without patching consumers' registry dependencies.
`Cargo.toml.upstream` preserves the original manifest. Unused standalone test
and benchmark targets are omitted; library unit tests remain available.

The optional `forklaunch-ajv-pointer-decoding` feature preserves literal slash
characters inside percent-encoded pointer tokens, matching AJV 8.20.0. Default
resolution retains the upstream RFC fragment behavior. Internal JSON pointers
are unchanged. The jsonschema dependency and pointer feature forward to the isolated
referencing package. Its separate optional `forklaunch-ajv-ref-siblings` feature
applies adjacent validation constraints in Drafts 4–7, matching AJV; upstream
behavior remains the default. No schema documents or literal data are rewritten.

Upstream library validation runs with `--no-default-features --features idna`
and `-- --skip validator::tests::validate_ref`. That single upstream test reads
`tests/suite/tests/draft7/ref.json`, which is absent from the published archive
(the JSON Schema Test Suite is an upstream Git submodule). All other 1,992
library tests run. ForkLaunch's own codec fixtures exercise local, recursive,
annotation and resource-scoped references independently against actual AJV.
The optional macro, Python, Ruby and HTTP retrieval adapters are outside the
ForkLaunch codec dependency profile.

Run standalone Cargo checks from the repository root with
`CARGO_TARGET_DIR=framework-rs/target` so build artifacts stay outside vendored
source and template directories.

The optional `forklaunch-static-anchor-resolution` feature keeps `$ref` to a
`$dynamicAnchor` static, while `$dynamicRef` still resolves through dynamic
scope (JSON Schema 2020-12 §8.2.3.2). Native codecs opt in. The referencing
resolver exposes `lookup_static`; the validator caches named references by
their resolved target URI and pointer, so static and dynamically rebound targets
do not share an incorrect cache entry. Pointer-reference aliases retain their
upstream identity to preserve embedded-resource and recursive-reference behavior.
A feature-gated regression covers static and dynamic references to the same
anchor; the full upstream library suite also runs with the feature enabled.
Default feature behavior remains unchanged.

The `forklaunch-ajv-ref-siblings` feature also preserves legacy `id`/`$id`
scopes beside `$ref`, matching AJV in Drafts 4–7. It now forwards to referencing;
default upstream resolution still ignores these IDs. Six actual AJV/native
cases distinguish sibling IDs that change a relative-reference base. The
referencing compatibility suite passes 151 tests.

The combined JSON Schema compatibility profile passes 1,986 library tests.
Seven upstream assertions intentionally require ignored legacy siblings and
therefore do not apply to this opt-in profile: the three
`ref_siblings_produce_no_annotations` cases, `draft7_ignores_pins_beside_a_reference`,
`unpinned_value_is_not_dispatched::test_case_3_const_beside_a_reference_in_draft_7`,
`absolute_keyword_location_in_nested_resource::test_case_10_draft_7_id_beside_ref_names_no_resource`,
and `custom_meta_schema_preserves_underlying_draft_behavior`. CI skips those
only in the compatibility profile, in addition to the archive-missing fixture
noted above. The default profile still runs every one of these assertions.

The optional `forklaunch-legacy-fragment-ids` feature resolves Draft-4/6/7 IDs
combining a URI/path with a named fragment as exact bundled aliases, matching
AJV. It does not invent a bare resource or a JSON-pointer base for those aliases.
Nested fragment-only IDs retain the enclosing lexical URI. Borrowed and owned
registries register those aliases, and the retrieval pass recognizes already
indexed exact targets without fetching their fragmentless URI. Native codecs
opt in through jsonschema's forwarded feature. Named-reference compilation and
cache identity preserve distinct full alias URIs, including aliases sharing a
path. Feature-gated resolver/validator regressions cover recursion, nested
anchors, distinct cached targets and unavailable bare/pointer resources.
Default upstream behavior remains unchanged.

The legacy fragment compatibility profile also indexes exact pointer-shaped and
percent-encoded aliases. Exact registered IDs take precedence over document
pointers, encoded delimiters stay distinct from literal delimiters, and ordinary
fragment-only names retain their existing resource scope. The registry's classic
schema-object analysis now honors `forklaunch-ajv-ref-siblings` when registering
IDs beside references, matching the existing ID extraction/compiler policy.
Borrowed/owned registry regressions and actual AJV/native/generated comparisons
cover these paths. URI syntax checks still reject malformed source references.

The optional runtime `with_schema_rewriter` hook adapts assertion nodes at their
original draft, location and resource scope. The default is absent. ForkLaunch
uses it only for multipart opaque-file validation; JSON/socket validation keeps
the original profile. Source registries, references and literal metadata remain
unchanged. Evaluated-item/property tracking uses the same adapted assertions,
and source-only optimizations are disabled for this profile. Rewritten values
stay owned throughout compilation so pointer-keyed caches cannot reuse freed
child addresses. The hook must be deterministic and cannot introduce resources.
Regression tests cover scoped references, evaluated properties, discriminator
and item optimizations, callback errors and unchanged default validation.

`const`, `enum` and `uniqueItems` compare objects by member set rather than
iteration order (`src/keyed_cmp.rs`). Upstream `jsonschema-value` zips object
members, which is only correct for sorted maps; forklaunch-core enables
`serde_json/preserve_order`, so equal objects with different key order were
rejected (`const`/`enum`) or counted as distinct (`uniqueItems`). Comparisons
without objects keep the backend's `equals_value`/`is_unique` fast path.
