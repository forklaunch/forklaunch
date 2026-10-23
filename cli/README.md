# forklaunch

Launch faster with forklaunch - a CLI for scaffolding TypeScript, Rust, and mixed service workspaces.

## Usage

```bash
forklaunch init application my-app
```

```bash
forklaunch init service my-service
```

```bash
forklaunch init library my-library
```

## Rust workspaces

```bash
forklaunch init application my-app --language rust --path ./my-app
forklaunch init service notes --language rust --path ./my-app
forklaunch init worker insights --language rust --path ./my-app
```

New Rust workspaces use `src/<service>`, `src/<worker>`, `src/core/<library>`,
and `src/client-sdk/{typescript,rust}`. Each Rust project has its own Cargo
manifest and Make targets. Services separate routes, controllers, schemas,
domain services, persistence, and startup; `main.rs` and `lib.rs` are entry/export
surfaces. Native Axum routers use ForkLaunch transport security and telemetry
wrappers. Service-owned contract crates generate TypeScript SDKs during Cargo
builds. Registered legacy projects retain their existing paths.

Framework and blueprint crates come from crates.io at the versions recorded in
the `[framework]` table of `.forklaunch/manifest.toml`; `--framework-path`
points an application at a local framework-rs checkout instead. Modules copy
their HTTP layer into `api/http`, as TypeScript modules copy their controllers.
