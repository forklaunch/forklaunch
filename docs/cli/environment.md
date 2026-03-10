---
title: Environment Command
category: CLI Reference
description: Manage environment variables across workspace projects.
---

## Overview

The `environment` command (alias: `env`) manages environment variables across all projects in your ForkLaunch workspace, ensuring consistency and validation.

## Usage

```bash
forklaunch environment <subcommand> [options]
# or use the alias
forklaunch env <subcommand> [options]
```

## Subcommands

### validate

Validates environment variables across all services and workers in the workspace.

```bash
forklaunch environment validate
```

**What it does:**
- Scans all services and workers for environment variable usage
- Checks that all required environment variables are defined
- Validates .env files exist where needed
- Reports missing or misconfigured environment variables

**Example:**
```bash
$ forklaunch env validate

[INFO] Validating environment variables...
[INFO] Found 3 services, 2 workers
[OK] All environment variables are properly configured

# If there are issues:
[ERROR] Missing environment variables:
  - SERVICE_payments: DATABASE_URL (required)
  - WORKER_email-processor: REDIS_URL (required)
```

### sync

Synchronizes environment variables from platform configuration to local .env files.

```bash
forklaunch environment sync [options]
```

**Options:**
- `-n, --dry-run` - Show what would be done without making changes

**What it does:**
- Finds missing environment variables across workspace projects
- Adds missing variables to appropriate `.env` files with blank values
- Respects the `.env` hierarchy by placing common variables in root `.env.local`

**Example:**
```bash
$ forklaunch env sync

[INFO] Syncing missing environment variables...
[OK] Added DB_HOST to payments/.env.local
[OK] Added REDIS_URL to .env.local
[OK] Environment variables synced successfully

# Preview only
$ forklaunch env sync --dry-run
```

## Common Workflows

### Check Environment Configuration

Before deploying or running locally:

```bash
forklaunch env validate
```

### Sync Missing Variables

Add blank entries for any missing environment variables:

```bash
forklaunch env sync
```

## Best Practices

1. **Validate Before Commit**: Run `forklaunch env validate` before committing changes
2. **Never Commit .env Files**: Add `.env` to `.gitignore`
3. **Use Platform for Secrets**: Store sensitive values in ForkLaunch platform, not in code
4. **Sync Regularly**: Keep local environment in sync with platform configuration
5. **Environment-Specific Values**: Use different values per environment (dev/staging/prod)

## Environment Variable Scopes

ForkLaunch automatically determines environment variable scopes:

- **Application-level**: Variables shared across all services/workers
- **Service-level**: Variables specific to a single service
- **Worker-level**: Variables specific to a single worker

The `validate` command ensures variables are properly scoped.

## Related Commands

- [`forklaunch integrate`](/docs/cli/integrate.md) - Link local app with platform
- [`forklaunch deploy create`](/docs/cli/deploy.md) - Deploy with environment configuration
- [`forklaunch config`](/docs/cli/config.md) - Manage application configuration

## See Also

- [Environment Management Guide](/docs/guides/environment-management.md)
- [Local Development](/docs/local-development.md)
