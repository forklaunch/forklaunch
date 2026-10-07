---
title: Sync Command
category: CLI Reference
description: Synchronize application directories with generated artifacts, environment variables, and configurations.
---

## Overview

The `sync` command updates your application code to reflect changes in manifest, configuration, or infrastructure definitions. It ensures generated code, types, configurations, and environment variables stay in sync with your project structure.

Sync performs comprehensive environment variable discovery by scanning all `.ts` source files for both `getEnvVar('VAR_NAME')` calls and `process.env.VAR_NAME` usage. It then synchronizes these variables across `.env.template` files, `.env.local` files, and `docker-compose.yaml` environment sections.

## Usage

```bash
forklaunch sync <target> [options]
```

## Subcommands

### all

Syncs all services, workers, and libraries in the workspace.

```bash
forklaunch sync all [options]
```

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `-c, --confirm` - Skip confirmation prompts
- `-P, --prompts <JSON>` - JSON object with pre-provided answers for prompts

**What it does:**
- Updates all generated code across the workspace
- Regenerates TypeScript types and interfaces
- Updates package dependencies
- Synchronizes docker-compose.yaml (including environment variables)
- Generates `.env.template` files with categorized env vars
- Syncs `.env.local` files with missing env vars
- Generates a root `.env.template` for application-scoped vars
- Detects and removes orphaned projects from artifacts

**Example:**
```bash
$ forklaunch sync all

[INFO] Scanning modules directory: src/modules
[INFO] Processing: billing
[INFO] Detected as: service
[INFO] Syncing service...
[OK] Generated .env.template for billing
[OK] Generated root .env.template
[INFO] Added 2 env var(s) to docker-compose service 'billing': OTEL_EXPORTER_OTLP_ENDPOINT, PLATFORM_MANAGEMENT_URL
[OK] Synchronized docker-compose environment variables
[OK] Sync all completed
```

### service

Syncs a specific service.

```bash
forklaunch sync service <service-name> [options]
```

**Arguments:**
- `<service-name>` - Name of the service to sync

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `-P, --prompts <JSON>` - JSON object with pre-provided answers for prompts

**What it does:**
- Regenerates service interfaces and types
- Updates service configuration files
- Refreshes OpenAPI specification for the service
- Updates service dependencies

**Example:**
```bash
$ forklaunch sync service payments

[INFO] Syncing service: payments
[OK] Regenerated interfaces
[OK] Updated configuration
[OK] Refreshed OpenAPI spec
[OK] Service 'payments' synced successfully
```

### worker

Syncs a specific worker.

```bash
forklaunch sync worker <worker-name> [options]
```

**Arguments:**
- `<worker-name>` - Name of the worker to sync

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `-P, --prompts <JSON>` - JSON object with pre-provided answers for prompts

**What it does:**
- Regenerates worker interfaces and types
- Updates worker configuration
- Refreshes queue/event definitions
- Updates worker dependencies

**Example:**
```bash
$ forklaunch sync worker email-processor

[INFO] Syncing worker: email-processor
[OK] Regenerated interfaces
[OK] Updated queue configuration
[OK] Refreshed event definitions
[OK] Worker 'email-processor' synced successfully
```

### library

Syncs a specific library.

```bash
forklaunch sync library <library-name> [options]
```

**Arguments:**
- `<library-name>` - Name of the library to sync

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `-P, --prompts <JSON>` - JSON object with pre-provided answers for prompts

**What it does:**
- Regenerates library exports and types
- Updates library configuration
- Refreshes package.json
- Updates library dependencies

**Example:**
```bash
$ forklaunch sync library shared-types

[INFO] Syncing library: shared-types
[OK] Regenerated exports
[OK] Updated package.json
[OK] Library 'shared-types' synced successfully
```

## Environment Variable Scanning

Sync performs deep environment variable discovery across your codebase:

### What Gets Scanned

1. **`registrations.ts` files** - Extracts `getEnvVar('VAR_NAME')` calls from each project's registrations file
2. **All `.ts` source files** - Recursively scans for `process.env.VAR_NAME` usage, excluding `node_modules`, `.d.ts` files, and `dist` directories

### Variable Scoping

Discovered env vars are automatically categorized into scopes:

| Scope | Criteria | Location |
|-------|----------|----------|
| **Application** | Used by 2+ projects, or from `core`/`monitoring` projects | Root `.env.template` and `.env.local` |
| **Service** | Used by a single service project | Per-service `.env.template` and `.env.local` |
| **Worker** | Used by a single worker project | Per-worker `.env.template` (symlinked) |

### Automatic Scope Promotions

Certain variables are automatically promoted to **Application** scope regardless of where they're used:

- **Observability vars**: `OTEL_*`, `LOKI_*`, `TEMPO_*`, `PROMETHEUS_*` - always application-scoped since monitoring is shared infrastructure
- **Inter-service URL vars**: Variables matching `{SERVICE_NAME}_{URL|URI|FQDN|HOST}` (e.g., `PLATFORM_MANAGEMENT_URL` when `platform-management` is a known project) - these reference other services and belong at the application level

### Generated Files

| File | Contents |
|------|----------|
| `<root>/.env.template` | Application-scoped vars only, organized by category |
| `<service>/.env.template` | Service-specific vars (application vars excluded) |
| `<worker>/.env.template` | Symlink to parent service's `.env.template` |
| `<root>/.env.local` | Missing application-scoped vars (added with empty values) |
| `<service>/.env.local` | Missing service-specific vars (added with empty values) |

### Docker-Compose Synchronization

Sync also updates `docker-compose.yaml` environment sections:

- Adds missing env vars discovered from code to the corresponding service/worker entries
- Adds application-scoped vars to all service/worker entries
- New vars are added with empty string values for you to fill in
- Existing values are never overwritten

## When to Use Sync

### After Manifest Changes

After modifying `.forklaunch/manifest.toml`:

```bash
# Edit manifest
vim .forklaunch/manifest.toml

# Sync to apply changes
forklaunch sync all
```

### After Schema Changes

After updating Zod schemas or API definitions:

```bash
# Edit user schema
vim src/modules/users/domain/schemas/user.schema.ts

# Sync to regenerate types
forklaunch sync service users
```

### After Adding Environment Variables

After adding new `getEnvVar()` or `process.env` usage in code:

```bash
# Added new env var usage in code
forklaunch sync all

# Check what was added
git diff docker-compose.yaml
git diff .env.template
```

### After Adding Dependencies

After adding external dependencies to your services:

```bash
# Added new dependencies in code
forklaunch sync all

# Check updated configuration
cat docker-compose.yaml
```

### After Project Changes

After using `forklaunch change` commands:

```bash
# Change service configuration
forklaunch change service payments --database mongodb

# Sync to apply changes
forklaunch sync service payments
```

### After Pulling Code

After pulling changes from git that affect project structure:

```bash
git pull origin main

# Sync to update local generated code
forklaunch sync all
```

## What Gets Synchronized

### Generated Code

- TypeScript interfaces and types
- Schema validators
- API route definitions
- Internal SDK functions

### Configuration Files

- `package.json` dependencies
- TypeScript configuration
- ESLint and Prettier configs
- Docker configurations

### Environment Variables

- Per-service `.env.template` files (service-scoped vars)
- Root `.env.template` file (application-scoped vars)
- `.env.local` files (missing vars added with empty values)
- `docker-compose.yaml` environment sections (missing vars added)

### Development Configuration

- `docker-compose.yaml`
- Database migration files
- Worker configurations

### Documentation

- OpenAPI specifications
- AsyncAPI specifications
- Internal API documentation

## Sync vs. Other Commands

### Sync vs. Change

- **`change`**: Modifies project structure or configuration
- **`sync`**: Applies those changes to generated code

```bash
# Change modifies configuration
forklaunch change service users --add-auth

# Sync applies the changes
forklaunch sync service users
```

### Sync vs. Depcheck

- **`sync`**: Updates generated code and configurations
- **`depcheck`**: Analyzes and updates npm dependencies

```bash
# Sync updates project structure
forklaunch sync all

# Depcheck ensures dependencies are aligned
forklaunch depcheck
```

### Sync vs. Release

- **`sync`**: Local development synchronization
- **`release`**: Creates a release manifest for deployment (also syncs internally)

Note: `forklaunch release create` automatically runs sync before generating the release manifest. During release, localhost values in env vars are filtered out from passthrough values.

## Common Workflows

### Complete Sync Workflow

```bash
# Make changes
vim .forklaunch/manifest.toml

# Sync changes
forklaunch sync all

# Verify changes
git diff

# Test locally
pnpm dev

# Commit if everything works
git add .
git commit -m "Updated configuration"
```

### Targeted Sync

```bash
# Only sync specific service
forklaunch sync service payments

# Or multiple specific targets
forklaunch sync service users
forklaunch sync service payments
forklaunch sync worker email-processor
```

### Post-Pull Sync

```bash
# Pull latest code
git pull origin main

# Install dependencies
pnpm install

# Sync to update generated code
forklaunch sync all

# Start development
pnpm dev
```

### CI/CD Non-Interactive Sync

```bash
# Use --confirm to skip interactive prompts
forklaunch sync all --confirm
```

## Troubleshooting

### Sync Conflicts

If sync detects conflicts:

```bash
[ERROR] Sync conflict detected in users/domain/interfaces/user.interface.ts
[INFO] Manual changes detected. Backup created at:
  users/domain/interfaces/user.interface.ts.backup
[INFO] Regenerated file created at:
  users/domain/interfaces/user.interface.ts
```

**Solution**: Review both files and merge manually if needed.

### Missing Dependencies

```bash
[ERROR] Missing required dependency: @forklaunch/core
[INFO] Run: pnpm install
```

**Solution**: Install dependencies first, then sync.

### Permission Issues

```bash
[ERROR] Cannot write to src/modules/users/
```

**Solution**: Check file permissions and ensure you have write access.

### Orphaned Projects

If sync finds projects in the manifest that no longer have corresponding directories:

```bash
[WARN] Found 1 orphaned project(s) in manifest:
  - old-service
Remove these orphaned projects from all artifacts? (y/N)
```

**Solution**: Confirm removal to clean up stale references, or skip if the directory was temporarily removed.

## Best Practices

1. **Sync After Every Structure Change**: Run sync after modifying manifest or project structure
2. **Sync Before Building**: Always sync before building for production
3. **Review Sync Changes**: Check `git diff` after sync to understand what changed
4. **Sync in CI/CD**: Include sync in your CI/CD pipeline with `--confirm` flag
5. **Commit Generated Code**: Commit synced code to version control for team consistency
6. **Check Root .env.template**: After sync, review the root `.env.template` for application-scoped vars that need values

## Performance Tips

- Use targeted sync (`service`, `worker`, `library`) instead of `all` for faster iteration
- Use `--path` to sync from anywhere in the workspace

## Related Commands

- [`forklaunch change`](/docs/changing-projects.md) - Modify project structure
- [`forklaunch depcheck`](/docs/cli/depcheck.md) - Check dependencies
- [`forklaunch init`](/docs/cli/init.md) - Initialize new projects
- [`forklaunch release`](/docs/cli/release.md) - Create release manifests

## See Also

- [Project Structure](/docs/guides/project-structure.md)
- [Generated Code](/docs/guides/generated-code.md)
- [Configuration Management](/docs/guides/configuration.md)
