# Sync Command Helper

You are helping the user run and troubleshoot `forklaunch sync` commands. This skill covers the sync workflow for the ForkLaunch CLI.

## What Sync Does

The `forklaunch sync` command synchronizes application code with generated artifacts:

1. **Project discovery** - Scans the modules directory, detects project types (service/worker/library), and syncs to manifest, docker-compose, runtime config, client SDK, and tsconfig
2. **Orphan cleanup** - Detects projects in manifest that no longer exist on disk and offers removal
3. **Environment variable scanning** - Scans all `.ts` source files for `getEnvVar('VAR')` and `process.env.VAR` usage
4. **Env var scoping** - Categorizes vars as Application, Service, or Worker scope:
   - Multi-project vars or core/monitoring vars -> Application scope
   - `OTEL_*`, `LOKI_*`, `TEMPO_*`, `PROMETHEUS_*` -> Application scope (observability)
   - `{SERVICE_NAME}_{URL|URI|FQDN|HOST}` matching a known project -> Application scope (inter-service)
   - Single-project vars -> Service or Worker scope
5. **Template generation** - Creates per-service `.env.template` (service-scoped vars) and root `.env.template` (application-scoped vars)
6. **Local env sync** - Adds missing vars to `.env.local` files (root for app-scoped, per-service otherwise)
7. **Docker-compose sync** - Adds missing discovered env vars to docker-compose.yaml environment sections

## Key Files

- CLI entry point: `cli/src/sync/all.rs` (SyncAllCommand)
- Service sync: `cli/src/sync/service.rs`
- Worker sync: `cli/src/sync/worker.rs`
- Library sync: `cli/src/sync/library.rs`
- Artifact sync: `cli/src/core/sync/artifacts.rs`
- Env var discovery: `cli/src/core/ast/infrastructure/env.rs`
- Env scoping: `cli/src/core/env_scope.rs`
- Env templates: `cli/src/core/env_template.rs`
- Docker-compose: `cli/src/core/docker.rs` (sync_docker_compose_env_vars)

## Commands

```bash
# Sync everything
forklaunch sync all

# Non-interactive (CI/CD)
forklaunch sync all --confirm

# Sync specific targets
forklaunch sync service <name>
forklaunch sync worker <name>
forklaunch sync library <name>

# Custom path
forklaunch sync all --path /path/to/app
```

## When the User Asks for Help

If the user wants to:
- **Add a new env var**: Tell them to add `getEnvVar('VAR_NAME')` or `process.env.VAR_NAME` in their code, then run `forklaunch sync all`. The var will be picked up and added to `.env.template`, `.env.local`, and docker-compose.
- **Debug missing env vars**: Check `cli/src/core/ast/infrastructure/env.rs` - the `find_all_env_vars` function scans registrations.ts and all .ts files.
- **Change env var scope**: Check `cli/src/core/env_scope.rs` - `determine_env_var_scopes` handles scope assignment and promotions.
- **Fix docker-compose sync**: Check `cli/src/core/docker.rs` - `sync_docker_compose_env_vars` adds missing vars to compose environment sections.
- **Run sync in the CLI codebase**: Use `cargo run -- sync all` from the `cli/` directory, or build first with `cargo build --release`.

## Building and Testing

```bash
# Build
cd cli && cargo build

# Run tests
cargo test

# Run specific env-related tests
cargo test env_vars
cargo test env_scope
cargo test env_template
cargo test should_passthrough
```
