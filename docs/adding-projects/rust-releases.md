# Rust crate releases

The [Rust Crate Release workflow](../../.github/workflows/rust_release.yml)
publishes only the 13 framework crates, including their three required protocol
forks, in dependency order. Local folders retain short names; published names use
`forklaunch-`. Run `python3 scripts/rust_release.py plan` for the release inventory.
`plan --scope all` also inventories the 28 blueprint packages for development;
publication rejects that scope and any report containing blueprint packages.
Sample crates marked `publish = false` are excluded. Pull requests validate packages;
manual runs also validate by default. Uploading requires selecting an existing
`rust-vVERSION` tag and explicitly enabling the `publish` input.

The [`rust-v0.1.0` release](https://github.com/forklaunch/forklaunch/releases/tag/rust-v0.1.0)
is published: all 13 downloaded crate archives match the verified release.
`publication.json` records each registry checksum. The CLI is validated separately
and is not published by this framework release.

## Local validation

Python 3.12+, stable Rust and Node 22 are required. Use an output directory outside
the checkout so release artifacts do not dirty the source tree:

```sh
python3 -m unittest discover -s scripts -p test_rust_release.py -v
python3 scripts/rust_release.py plan
python3 scripts/rust_release.py prepare --output /tmp/forklaunch-rust-release
python3 scripts/rust_release.py verify \
  --output /tmp/forklaunch-rust-release \
  --target-dir /tmp/forklaunch-rust-release-target
```

Preparation requires a new output directory and a clean checkout. For local
development, `prepare --allow-dirty` is available; a dirty release cannot publish.
The report contains the commit, ordered package inventory and SHA-256 checksums.
Preparation uses Cargo's versioned path dependency packaging support, excludes
lockfiles, and defers build verification until all unpublished dependencies can
be installed together. See [Cargo dependency locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations).

Verification serves a loopback sparse registry containing only the owned packages.
It changes owned dependency registry metadata in staging copies, preserves source
files and original archives, and obtains external dependencies from crates.io.
Every public crate must resolve from the test registry. A separate consumer builds
their default features, with JSON Schema/referencing protocol defaults disabled, checked bounded regex,
core build/schema/mapper/PostgreSQL/OTLP features and Axum MCP/native features enabled. It roundtrips literal JSON protocol-marker keys and runs real HTTP/MCP
discovery, invocation, Origin rejection, listener configuration, startup rejection
and graceful shutdown checks. This is an installation and startup check; optional
adapter features and runtime behavior retain their own parity tests.

## JSON parser

Applications use upstream `serde_json`; no workspace patch or parser artifact is
required. Releases through v0.1.1 attached a patched parser archive; applications
generated with those releases have it removed by `forklaunch sync`, which keeps any
patch the application added itself. Axum and SQLx enable `raw_value`, so an object
whose first key is exactly `$serde_json::private::RawValue` is read as embedded raw
JSON, as in any Axum or SQLx application. See the
[boundary audit](../../plan/RUST-JSON-BOUNDARY-AUDIT.md).

## Blueprint releases

The blueprint publishes like the TypeScript blueprint: interface and implementation
crates only. Module HTTP layers (`*-axum`) are copied into applications by
generation; shared module types and gateway clients are framework APIs
(`forklaunch_core::domain`, `forklaunch_core::http`). Blueprint crates pin exact
framework versions, so publish the framework first.

```sh
python3 scripts/rust_release.py plan --scope blueprint
python3 scripts/rust_release.py prepare --scope blueprint --output /tmp/blueprint-release
# Against published framework crates:
python3 scripts/rust_release.py verify --output /tmp/blueprint-release --target-dir /tmp/blueprint-target
# Or against an unpublished, prepared framework release:
python3 scripts/rust_release.py verify --output /tmp/blueprint-release --target-dir /tmp/blueprint-target --framework-release /tmp/framework-release
```

Publish with tag `rust-blueprint-vVERSION` and the release workflow's `scope:
blueprint` input. Publication refuses to start until every pinned framework
version is on crates.io. Generated applications record `blueprint_version` in
the `[framework]` table of `.forklaunch/manifest.toml` and depend on these crates by version;
no blueprint source is copied.

## Publishing

Create the versioned tag and corresponding public GitHub release only after release
review, configure the repository's `CARGO_REGISTRY_TOKEN` with access to the intended
crate names, then run the workflow on that tag with `publish` enabled. The workflow
first validates, uploads the reports to the existing release,
and then publishes the framework crates. Retrying the same release replaces its
verification assets only after the tagged source and artifact checks pass.

The same checks can be reviewed locally without uploading:

```sh
python3 scripts/rust_release.py publish \
  --output /tmp/forklaunch-rust-release --tag rust-v0.1.0
```

Only adding `--upload` permits Cargo uploads. Preflight checks the clean checkout,
tag, commit, complete package graph, all artifact checksums and the matching
successful installation/startup receipt. Receipts are CI evidence, not signed
attestations; publish from trusted workflow artifacts. Cargo publishes sequentially
from the checked-out source, stopping at the first failure. It repackages crates
and may add lockfiles, so upload archive bytes need not equal the earlier preparation
archives. `--no-verify` avoids repeating individual package builds after the combined
registry consumer has passed. [Cargo publication behavior](https://doc.rust-lang.org/cargo/commands/cargo-publish.html)
also includes waiting for newly published versions to become available.

Publication is not atomic across crates. The publisher waits for each version in
the registry, compares its downloaded archive with the verified package (allowing
only Cargo's generated lockfile to differ), and writes `publication.json` after
each success. Reruns resume by verifying existing versions; mismatched or yanked
versions stop the release. Cargo uses its configured credential provider, including
`cargo login`, registry-specific environment tokens or CI credentials. Publication and external application installation each have their own receipt.
Optional adapter and deployment checks remain separate from this release verification.

## Native framework release acceptance

Encryption is implemented in core; native worker support belongs to the existing
blueprint worker-interface crate. Framework releases publish framework crates only;
the blueprint follows in its own `rust-blueprint-v*` release, and generated workers
depend on the published worker-interface and implementation crates.
Publication order includes runtime, build and versioned development dependencies.
The macro crate tests its public facade through an unversioned local development
path, which Cargo omits from published metadata. This repository-only backedge
avoids a first-publication cycle; its target is still validated and repository
tests retain the full facade. A versioned development backedge fails release planning.

A prepared archive or passing dependency inventory is not a public release. The
public-registry acceptance suite verifies the following independently:

- Exact framework crate versions are available in the intended registry, including
  the HTTP native feature used by generated apps. Blueprint worker adapters remain local.
- A fresh CLI-generated app builds without `vendor/framework` and without workspace
  source paths. Natural local dependency aliases may retain the published
  `forklaunch-` package names.
- Contract compilation generates OpenAPI, TypeScript and Rust SDKs. The generated
  shared Docker image starts service and worker producer HTTP/MCP surfaces, rejects
  unauthenticated producer calls, and includes the worker consumer binary. Separate
  encrypted worker and infrastructure runtime suites exercise transport behavior.

The current CLI defaults new applications to exact framework release versions.
Existing application
source choices are preserved. `--allow-dirty` is for local release validation and
cannot produce a publishable receipt.

Current publication readiness is recorded in
[`RUST-PUBLISH-READINESS.json`](../../plan/RUST-PUBLISH-READINESS.json). The current
release inventory is 13 framework packages; older 41- or 53-package staging proofs
do not validate this restricted publication graph. Registry-mode worker generation still retains
its blueprint worker-interface snapshot.

## Explicit application dependency modes

Initialization defaults to the framework and blueprint releases in
[`cli/src/core/cargo_toml/cargo_toml_constants.rs`](../../cli/src/core/cargo_toml/cargo_toml_constants.rs),
which also holds the third-party crate versions generated Cargo.toml files use
(templates and generators leave those versions out). `check_blueprint_deps`
fails CI when that file drifts from framework-rs or blueprint-rs, and `--fix`
updates it. To select another release, pass `--framework-version X.Y.Z` to
`forklaunch init application NAME`, or `--framework-path` for a local framework-rs
checkout. Application initialization is language-neutral;
choose `--language rust` on subsequent service/worker commands. Both SDK targets
are the default. The saved preference pins direct framework dependencies to that
exact version when a Rust project is added, preserving features and local aliases
without copying the full framework snapshot. There are no application presets.
Generation does not assert the version exists.

For host framework development, use `--framework-path /absolute/path/framework-rs`
instead. This deliberately produces host-local path dependencies. Before attaching
Rust, the CLI requires an application-owned `Dockerfile.rust` integrating that
source; managed images do not automatically supply a local checkout. The version, path and snapshot flags are mutually
exclusive. Source selection is recorded in the `[framework]` table of `.forklaunch/manifest.toml`
and applied to subsequently generated native Cargo manifests. Blueprint module
implementations remain development snapshots; their framework dependencies use
the selected source.

After selecting a dependency mode, run `cargo generate-lockfile` and commit the resulting
lock before `cargo build --locked` or Docker startup. A source-snapshot lock cannot
prove registry resolution and is removed during initialization. Never substitute
a local path build for a clean registry-install release gate.

### Why renaming the parser is insufficient

Publishing the patched parser as a different crate does not redirect transitive
upstream `serde_json` dependencies. Axum, MCP adapters and SQL persistence would
then use a different `Value` type and potentially different marker-key semantics.
A Cargo patch entry for a differently named package does not transparently replace
all dependencies on the upstream package. Removing the required patch therefore
needs upstream adoption or an explicitly tested boundary/type migration across
those consumers. A package rename alone is not an acceptable release fix.

## HTTP SDK compiler consistency

Native service builds emit OpenAPI and discrete TypeScript exports under the
service's `sdk/typescript` directory. Its `sdk/rust` Cargo package compiles against
the same private contract host and includes generated Rust source from `OUT_DIR`.
The optional client aggregate depends only on these local SDK packages. Current contract artifacts carry
`sdkCompiler: forklaunch.sdk.http.v1`. For those artifacts, CLI SDK generation
invokes the same core compiler linked into the CLI, without spawning a Cargo host
or importing the running application.
Generation consumes the supplied artifact, including path/query/header schemas,
status codes and media types; it does not rebuild service contracts implicitly.
The CLI preserves project-local SDK packages and uses the same compiler for
explicit artifact-based exports.
`--check` stages expected output and reports drift without rewriting SDK files.
Older artifacts without a compiler marker continue through the legacy compiler.

Cargo compilation uses the private contract host and configured dependency
sources. Explicit CLI generation from
contract or OpenAPI artifacts uses the linked core compiler. Project-local
OpenAPI imports first pass through the core canonical adapter, so named schemas
and empty response bodies follow the same conventions as compilation.

The v0.1 native offline-export protocol emits standard OpenAPI. The CLI attaches
checked universal-SDK operation descriptors from the same freshly exported
contract before refreshing registered and requested output paths. Existing
descriptors must match; private routes stay excluded. This compatibility adapter
uses the framework compiler, matching ordinary build outputs and HTTP docs.
Calling the v0.1 raw export protocol directly does not add those SDK extensions.
