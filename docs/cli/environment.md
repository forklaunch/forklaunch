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
forklaunch environment validate [options]
```

**Options:**
- `-p, --path <path>` - Path to application root (optional, defaults to current directory)

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
- `-p, --path <path>` - Path to application root (optional)
- `-e, --environment <env>` - Environment to sync (development, staging, production)
- `-r, --region <region>` - Region to sync from

**What it does:**
- Fetches environment configuration from ForkLaunch platform
- Updates local .env files with platform values
- Preserves local-only variables
- Creates backup of existing .env files

**Example:**
```bash
$ forklaunch env sync --environment development --region us-west-2

[INFO] Fetching environment configuration...
[INFO] Syncing 12 variables to local .env files
[OK] Backed up existing .env to .env.backup
[OK] Updated .env files for:
  - services/payments/.env
  - services/users/.env
  - workers/email-processor/.env
[OK] Environment variables synced successfully
```

## Common Workflows

### Check Environment Configuration

Before deploying or running locally:

```bash
forklaunch env validate
```

### Sync Development Environment

Pull latest environment config from platform:

```bash
forklaunch env sync --environment development --region us-west-2
```

### Setup New Developer Machine

```bash
# Clone repository
git clone <repo>
cd <repo>

# Sync environment from platform
forklaunch login
forklaunch env sync --environment development --region us-west-2

# Start development
pnpm dev
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
