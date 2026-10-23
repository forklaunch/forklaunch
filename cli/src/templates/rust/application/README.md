# {{{name}}}

{{{description}}}

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo run -p <service-name>
# Run generated Compose services together, rebuilding Rust on source edits:
forklaunch dev
```

Services expose `/health`, `/openapi.json`, and the OpenAPI document at `/docs`. Configuration has no in-code defaults: `HOST`, `PORT`, `PROTOCOL`, `VERSION`, `DOCS_PATH`, `RUST_LOG` and `MCP_ENABLED` are required, and a missing or malformed setting stops startup with an error naming it. `forklaunch dev` supplies them from `docker-compose.yaml`; for `cargo run`, load the service `.env.local` (see `.env.example`). Services stop gracefully on Ctrl-C or SIGTERM.

## Contracts and SDKs

```sh
forklaunch openapi export --path .
forklaunch sdk generate --path . --target all --from contract
forklaunch sdk generate --path . --target all --from contract --check
```

Framework crates come from crates.io at the versions in the `[framework]` table of `.forklaunch/manifest.toml`. Initialization preserves existing source files.

The custom service starter uses an in-memory repository. Add service-owned database migrations and repositories before deploying application workloads. Database/Redis/Kafka worker variants retain their durable queue adapters.

## TypeScript SDKs from Rust contracts

New custom services generate `src/client-sdk/typescript/<service>/index.ts` and contract/OpenAPI JSON during `cargo check` or `cargo build`. Their `build.rs` calls the generator as a Rust library; it does not invoke Cargo recursively or start the service.

Define request/response DTOs with `#[forklaunch_core::contract::schema]` in `src/<service>/domain/schemas`, and build route bodies with `BodyContract::json_type::<YourDto>()`. Runtime handlers import those same types and contracts. Editing a shared contract triggers regeneration; unchanged outputs are not rewritten. `FORKLAUNCH_TYPESCRIPT_SDK_DIR` overrides the output directory for an individual service build.

Build-time contracts must be independent of runtime state. Add host build dependencies when shared contract code needs another library. Runtime-only route registration and existing built-in modules still use their explicit contract export workflow.

## Project structure

Each service and worker is a Cargo project directly under `src/<name>`, with `lib.rs` exporting modules and `main.rs` calling the bootstrapper. Custom services own `api/{controllers,routes,contracts}`, `domain/{schemas,types,services,mappers}`, `persistence` and `migrations`. The contract-only `domain/Cargo.toml` compiles `sdk.rs` for host build scripts without linking the service runtime.

Use `make -C src/<name> build`, `test`, `lint`, `fmt`, `docs`, or `run`. The optional `dev` task requires cargo-watch. Native HTTP composition installs ForkLaunch origin, request size, timeout and response policies around ordinary Axum routes. Configure `PUBLIC_ORIGIN` for browser access through a reverse proxy.
