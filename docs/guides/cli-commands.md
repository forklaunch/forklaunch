---
title: CLI Commands Reference
category: Development
description: Comprehensive reference for ForkLaunch CLI commands including sync, deploy, openapi, sdk, environment, and integrate operations.
---

## Overview

This guide provides detailed documentation for ForkLaunch CLI commands. These commands manage application artifacts, deployments, environment variables, and SDK configuration. The CLI is designed to streamline development workflows from local setup to platform deployment.

## Sync Command

The `sync` command synchronizes your application directories with application artifacts. It updates your manifest file based on the current project structure and detects project types, configurations, and metadata.

### Basic Usage

```bash
forklaunch sync <subcommand> [OPTIONS]
```

### Subcommands

#### sync all

Performs a full synchronization of all projects in the modules directory with the manifest.

**Usage:**
```bash
forklaunch sync all [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--path <PATH>` | `-p` | Path to application root (optional) |
| `--confirm-all` | `-y` | Skip interactive prompts and confirm all changes |
| `--help` | `-h` | Show help information |

**What It Does:**
- Scans the modules directory for all project folders
- Detects project types (service, worker, library, router)
- Identifies orphaned projects (in manifest but not on disk)
- Automatically detects service configurations, database settings, and infrastructure requirements
- Prompts for missing metadata (description, database type, etc.)
- Updates the manifest with detected configurations
- Handles orphaned project cleanup

**Examples:**

Basic sync of all projects:
```bash
forklaunch sync all
```

Sync all projects without interactive prompts:
```bash
forklaunch sync all --confirm-all
```

Sync from a specific path:
```bash
forklaunch sync all --path /path/to/application
```

**Workflow:**
1. Scans modules directory for project folders
2. Compares with existing manifest entries
3. Detects new and removed projects
4. For each project, analyzes configuration files to determine type and settings
5. Prompts for any missing or ambiguous information
6. Updates manifest with new entries and removes orphaned ones
7. Generates necessary configuration files

---

#### sync service

Synchronizes a specific service with artifacts.

**Usage:**
```bash
forklaunch sync service <SERVICE_NAME> [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--path <PATH>` | `-p` | Path to application root (optional) |
| `--help` | `-h` | Show help information |

**What It Does:**
- Analyzes service directory structure
- Detects HTTP route configurations
- Identifies database requirements
- Determines infrastructure dependencies
- Prompts for service metadata (description, database type)
- Updates service artifacts in manifest

**Examples:**

Sync a single service:
```bash
forklaunch sync service billing
```

Sync service from custom path:
```bash
forklaunch sync service payment --path ./apps
```

**Interactive Prompts:**
- Service description (optional)
- Database type (if database detected)
- Infrastructure configuration (Redis, S3, etc.)

---

#### sync worker

Synchronizes a specific worker with artifacts.

**Usage:**
```bash
forklaunch sync worker <WORKER_NAME> [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--path <PATH>` | `-p` | Path to application root (optional) |
| `--help` | `-h` | Show help information |

**What It Does:**
- Analyzes worker directory structure
- Detects worker type (BullMQ, Kafka, Celery, etc.)
- Identifies database and queue dependencies
- Determines infrastructure requirements
- Prompts for worker metadata
- Updates worker artifacts in manifest

**Examples:**

Sync a single worker:
```bash
forklaunch sync worker email-processor
```

Sync worker with custom path:
```bash
forklaunch sync worker notification-handler --path ./modules
```

**Supported Worker Types:**
- BullMQ (Redis-based job queue)
- Kafka (distributed streaming)
- Celery (async task queue)
- Custom workers

---

#### sync library

Synchronizes a specific library with artifacts.

**Usage:**
```bash
forklaunch sync library <LIBRARY_NAME> [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--path <PATH>` | `-p` | Path to application root (optional) |
| `--help` | `-h` | Show help information |

**What It Does:**
- Analyzes library exports and structure
- Updates library metadata in manifest
- Validates library configuration
- Prompts for description and metadata

**Examples:**

Sync a single library:
```bash
forklaunch sync library shared-utils
```

Sync library from custom path:
```bash
forklaunch sync library helpers --path ./packages
```

---

### Common Sync Workflows

**Initialize Fresh Application:**
```bash
# After creating a new ForkLaunch application
forklaunch sync all --confirm-all
```

**Add New Service and Sync:**
```bash
# Create service directory with configuration
mkdir -p modules/new-service/src
# Add service implementation files

# Sync to detect and register new service
forklaunch sync service new-service
```

**Detect Service Changes:**
```bash
# After modifying service configuration (adding database, routes, etc.)
forklaunch sync service service-name
```

**Cleanup and Reorganize:**
```bash
# Remove old services and resync manifest
forklaunch sync all
# CLI will detect missing directories and prompt for cleanup
```

---

## Deploy Command

The `deploy` command manages application deployments to AWS infrastructure through the ForkLaunch platform.

### Basic Usage

```bash
forklaunch deploy <subcommand> [OPTIONS]
```

### Subcommands

#### deploy create

Creates a new deployment of your application to a specified environment and region.

**Usage:**
```bash
forklaunch deploy create --release <VERSION> --environment <ENV> --region <REGION> [OPTIONS]
```

**Required Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--release <VERSION>` | `-r` | Release version to deploy (e.g., v1.0.0) |
| `--environment <ENV>` | `-e` | Environment name (staging, production, etc.) |
| `--region <REGION>` | - | AWS region (us-east-1, eu-west-1, etc.) |

**Optional Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--distribution-config` | - | Distribution strategy (centralized or distributed) |
| `--path <PATH>` | `-p` | Path to application root |
| `--no-wait` | - | Don't wait for deployment to complete |

**What It Does:**
1. Reads application manifest to get platform integration info
2. Validates application exists on platform
3. Creates deployment request with specified configuration
4. Monitors deployment progress (unless --no-wait is set)
5. Detects missing environment variables and prompts for values
6. Handles environment variable validation based on component metadata
7. Automatically retries after environment variables are provided

**Examples:**

Basic deployment:
```bash
forklaunch deploy create --release v1.0.0 --environment staging --region us-east-1
```

Production deployment:
```bash
forklaunch deploy create -r v1.0.0 -e production --region us-east-1
```

Non-blocking deployment (check status later):
```bash
forklaunch deploy create -r v1.0.0 -e staging --region us-east-1 --no-wait
```

Distributed deployment:
```bash
forklaunch deploy create -r v1.0.0 -e production --region us-east-1 --distribution-config distributed
```

**Interactive Environment Variables:**

When deployment detects missing environment variables:

```
[WARNING] Deployment blocked: Missing required environment variables

[INFO] Missing variables for service 'api':
  Enter value for DATABASE_URL (service:database-connection)
  Enter value for NODE_ENV
```

**Special Handling:**
- `NODE_ENV`: Interactive selection between development/production
- Port variables (ending with `_PORT`): Must be valid port numbers (0-65535)
- Duration variables (ending with `_SECONDS`, `_TIMEOUT`): Must be numeric
- Base64 encoded values: Validates format and length based on component requirements
- Empty values: Not allowed; validation prevents blank submissions

**Environment Variable Scopes:**

Environment variables can be scoped to:
- **Global**: Applied to all services and workers
- **Service-specific**: Applied to specific service
- **Worker-specific**: Applied to specific worker
- **Component property**: Linked to component configuration property

---

#### deploy destroy

Destroys application infrastructure in a specified environment and region.

**Usage:**
```bash
forklaunch deploy destroy --environment <ENV> --region <REGION> [OPTIONS]
```

**Required Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--environment <ENV>` | `-e` | Environment name |
| `--region <REGION>` | - | AWS region |

**Optional Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--mode` | - | Destroy mode: all (default) or preserve-data |
| `--path <PATH>` | `-p` | Path to application root |
| `--no-wait` | - | Don't wait for destruction to complete |

**What It Does:**
1. Validates application is integrated with platform
2. Creates destruction request with specified mode
3. Monitors destruction progress (unless --no-wait is set)
4. Cleans up AWS resources based on mode

**Destroy Modes:**
- `all`: Completely removes all infrastructure and data (default)
- `preserve-data`: Removes compute resources but preserves databases and storage

**Examples:**

Destroy staging environment:
```bash
forklaunch deploy destroy --environment staging --region us-east-1
```

Destroy with data preservation:
```bash
forklaunch deploy destroy -e staging --region us-east-1 --mode preserve-data
```

Non-blocking destruction:
```bash
forklaunch deploy destroy -e staging --region us-east-1 --no-wait
```

---

### Common Deploy Workflows

**Deploy New Release:**
```bash
# After pushing changes and creating release
forklaunch deploy create --release v1.1.0 --environment staging --region us-east-1

# After testing in staging
forklaunch deploy create --release v1.1.0 --environment production --region us-east-1
```

**Progressive Deployment:**
```bash
# Deploy to staging first
forklaunch deploy create -r v2.0.0 -e staging --region us-east-1

# Run tests in staging...

# Then deploy to production
forklaunch deploy create -r v2.0.0 -e production --region us-east-1
```

**Cleanup Temporary Environment:**
```bash
# Destroy testing environment but keep database
forklaunch deploy destroy -e testing --region us-east-1 --mode preserve-data

# Later: Fully remove all resources
forklaunch deploy destroy -e testing --region us-east-1 --mode all
```

---

## OpenAPI Command

The `openapi` command manages OpenAPI specifications for your services.

### Basic Usage

```bash
forklaunch openapi <subcommand> [OPTIONS]
```

### Subcommands

#### openapi export

Exports OpenAPI specifications from all services in your application.

**Usage:**
```bash
forklaunch openapi export [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--output <DIR>` | `-o` | Output directory (default: .forklaunch/openapi) |
| `--path <PATH>` | `-p` | Path to application root (optional) |
| `--help` | `-h` | Show help information |

**What It Does:**
1. Reads application manifest
2. Discovers all services with OpenAPI documentation
3. Exports specification files to output directory
4. Creates organized directory structure with service specs
5. Reports success with list of exported services

**Examples:**

Export to default location:
```bash
forklaunch openapi export
```

Export to custom directory:
```bash
forklaunch openapi export --output ./api-specs
```

Export from custom application root:
```bash
forklaunch openapi export --path ./apps/myapp --output ./specs
```

**Output Structure:**
```
.forklaunch/openapi/
├── service-a-openapi.json
├── service-b-openapi.json
├── service-c-openapi.json
└── ...
```

**Use Cases:**
- Generate API documentation from service definitions
- Create API contracts for client code generation
- Share specifications with API consumers
- Integrate with API gateways and documentation tools
- Enable contract-first development workflows

---

### Common OpenAPI Workflows

**Generate API Documentation:**
```bash
# Export specifications
forklaunch openapi export --output ./docs/api

# Generate HTML docs from specifications (with other tools)
# redoc-cli build ./docs/api/*.json
```

**API Client Generation:**
```bash
# Export specifications
forklaunch openapi export --output ./openapi-specs

# Generate TypeScript clients
# openapi-generator generate -i ./openapi-specs/*.json -g typescript
```

**API Gateway Configuration:**
```bash
# Export and configure in API gateway
forklaunch openapi export --output ./gateway-specs

# Upload to AWS API Gateway or other services
```

---

## SDK Command

The `sdk` command manages SDK modes for your application.

### Basic Usage

```bash
forklaunch sdk <subcommand> [OPTIONS]
```

### Subcommands

#### sdk mode

Changes the SDK mode for your application between generated and live modes.

**Usage:**
```bash
forklaunch sdk mode --type <TYPE> [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--type <TYPE>` | `-t` | SDK mode type: generated or live |
| `--dryrun` | `-n` | Show changes without applying them |
| `--help` | `-h` | Show help information |

**SDK Modes:**

**Generated Mode:**
- Uses pre-generated, compiled TypeScript definitions
- Fastest at runtime
- Type-safe SDK exports
- Generated tsconfig.json and VSCode tasks
- Best for production deployments
- No hot reloading of SDK types

**Live Mode:**
- Uses live TypeScript imports from project sources
- Enables real-time type updates
- Hot module reloading support
- Removes generated configuration files
- Better for development
- Slower at runtime due to compilation overhead

**What It Does:**
1. Reads application manifest
2. Detects current SDK mode from project structure
3. For generated mode:
   - Generates root tsconfig.json
   - Creates VSCode build tasks
   - Updates package.json files with type definitions
4. For live mode:
   - Removes generated configuration files
   - Removes type definitions from package.json
   - Restores development-friendly setup
5. Writes changes (or shows dry run results)

**Examples:**

Switch to generated mode:
```bash
forklaunch sdk mode --type generated
```

Switch to live mode:
```bash
forklaunch sdk mode --type live
```

Preview changes without applying:
```bash
forklaunch sdk mode --type generated --dryrun
```

---

### Common SDK Workflows

**Development Setup:**
```bash
# Start development with live SDK mode
forklaunch sdk mode --type live

# Work on services with hot type reloading
# Make changes and see type updates immediately
```

**Production Build:**
```bash
# Switch to generated mode for build
forklaunch sdk mode --type generated

# Build application with pre-generated types
npm run build
```

**Mode Switching:**
```bash
# Debug development issues with live mode
forklaunch sdk mode --type live

# Review changes without applying
forklaunch sdk mode --type live --dryrun

# Apply when ready
forklaunch sdk mode --type live
```

---

## Environment Command

The `environment` command manages environment variables across your workspace projects.

**Alias:** `env`

### Basic Usage

```bash
forklaunch environment <subcommand> [OPTIONS]
# or
forklaunch env <subcommand> [OPTIONS]
```

### Subcommands

#### environment validate

Checks all workspace projects for missing environment variables.

**Usage:**
```bash
forklaunch environment validate
# or
forklaunch env validate
```

**What It Does:**
1. Scans modules directory for all projects
2. Reads registrations.ts files to find environment variable references
3. Checks for corresponding .env files
4. Validates variables are defined across .env hierarchy
5. Reports missing and undefined variables
6. Provides scoping information (global, service-specific, worker-specific)

**Examples:**

Validate environment variables:
```bash
forklaunch environment validate
```

Alias form:
```bash
forklaunch env validate
```

**Output Example:**
```
Validating environment variables...
Workspace: /path/to/app
Modules path: /path/to/app/modules

3 projects found:
  - api-service
  - email-worker
  - utils-library

Validation Results:
✓ api-service: All variables defined
✗ email-worker: 2 missing variables
  - SMTP_PASSWORD (worker-specific)
  - QUEUE_WORKERS (worker-specific)
✓ utils-library: All variables defined
```

---

#### environment sync

Syncs missing environment variables by adding them with blank values to appropriate .env files.

**Usage:**
```bash
forklaunch environment sync [OPTIONS]
# or
forklaunch env sync [OPTIONS]
```

**Options:**
| Option | Short | Description |
| :----- | :---- | :---------- |
| `--dry-run` | `-n` | Show changes without applying them |
| `--help` | `-h` | Show help information |

**What It Does:**
1. Runs validation to find missing variables
2. Determines appropriate .env file for each variable
3. Adds missing variables with blank values
4. Respects .env hierarchy (places common variables in .env.local)
5. Reports all changes made

**Examples:**

Sync missing variables:
```bash
forklaunch environment sync
```

Preview changes:
```bash
forklaunch environment sync --dry-run
```

Alias form:
```bash
forklaunch env sync
```

**Output Example:**
```
Syncing environment variables...
Workspace: /path/to/app
Modules path: /path/to/app/modules

Running validation first...

Adding 4 missing variables:
  [✓] Added SMTP_PASSWORD to modules/email-worker/.env
  [✓] Added QUEUE_WORKERS to modules/email-worker/.env
  [✓] Added API_KEY to modules/api-service/.env.local
  [✓] Added DEBUG_MODE to .env.local

Sync completed successfully!
```

---

### Environment Variable Hierarchy

Variables are organized in a hierarchy:
1. **Global variables** (root .env.local) - used by all projects
2. **Service-specific** (modules/service-name/.env) - used by specific service
3. **Worker-specific** (modules/worker-name/.env) - used by specific worker
4. **Component properties** (linked to component configuration)

---

### Common Environment Workflows

**Setup New Environment:**
```bash
# Validate what variables are needed
forklaunch environment validate

# Add all missing variables with placeholders
forklaunch environment sync

# Fill in values
nano .env.local
nano modules/api-service/.env
```

**Onboard New Developer:**
```bash
# New developer clones repository
git clone <repo>
cd <app>

# Sync environment variables
forklaunch env sync

# Developer fills in their local values
nano .env.local
```

**Validate Before Deployment:**
```bash
# Check all variables are set before deploying
forklaunch environment validate

# Fix any missing variables
forklaunch environment sync
```

---

## Integrate Command

The `integrate` command links your local application with a ForkLaunch platform application.

### Basic Usage

```bash
forklaunch integrate --app <APP_ID> [OPTIONS]
```

### Options

| Option | Short | Description |
| :----- | :---- | :---------- |
| `--app <APP_ID>` | `-a` | Platform application ID (required) |
| `--path <PATH>` | `-p` | Path to application root (optional) |
| `--help` | `-h` | Show help information |

### What It Does

1. Validates you are logged in to the ForkLaunch platform
2. Checks that specified application exists on platform
3. Reads local application manifest
4. Updates manifest with platform application and organization IDs
5. Confirms integration and shows next steps

### Examples

Integrate with platform application:
```bash
forklaunch integrate --app app_123abc
```

Integrate from custom path:
```bash
forklaunch integrate --app app_123abc --path ./apps/myapp
```

### Manifest Updates

After integration, your `.forklaunch/manifest.toml` is updated with:

```toml
[application]
platform_application_id = "app_123abc"
platform_organization_id = "org_456def"
```

### Next Steps

After integration, you can:

```bash
# Create releases
forklaunch release create --version v1.0.0

# Deploy to platform
forklaunch deploy create --release v1.0.0 --environment staging --region us-east-1

# Manage deployments through platform UI
```

---

### Common Integration Workflows

**Connect Local App to Platform:**
```bash
# Create application on platform web UI (get APP_ID)

# In local repository
forklaunch integrate --app app_123abc

# Now ready to deploy
forklaunch release create --version v1.0.0
forklaunch deploy create --release v1.0.0 --environment staging --region us-east-1
```

**Link Existing Application:**
```bash
# Clone repository
git clone <repo>
cd <app>

# Get application ID from platform
# Link to platform
forklaunch integrate --app app_xyz789

# Sync projects
forklaunch sync all

# Deploy
forklaunch deploy create --release v1.0.0 --environment staging --region us-east-1
```

---

## Best Practices

### Development Workflow

1. **Initialize and Sync:**
   ```bash
   forklaunch init application my-app --path ./my-app
   forklaunch sync all
   ```

2. **Add Components:**
   ```bash
   forklaunch init service billing
   forklaunch sync service billing
   ```

3. **Manage Environment Variables:**
   ```bash
   forklaunch environment validate
   forklaunch environment sync
   # Fill in values
   ```

4. **Use Live SDK Mode:**
   ```bash
   forklaunch sdk mode --type live
   ```

5. **Test Locally:**
   ```bash
   npm run dev
   ```

### Deployment Workflow

1. **Prepare for Deployment:**
   ```bash
   forklaunch integrate --app <APP_ID>
   forklaunch sync all
   forklaunch environment validate
   ```

2. **Create Release:**
   ```bash
   forklaunch release create --version v1.0.0
   ```

3. **Switch to Generated Mode:**
   ```bash
   forklaunch sdk mode --type generated
   ```

4. **Deploy to Staging:**
   ```bash
   forklaunch deploy create --release v1.0.0 --environment staging --region us-east-1
   ```

5. **Test in Staging:**
   ```bash
   # Run integration tests, manual testing
   ```

6. **Deploy to Production:**
   ```bash
   forklaunch deploy create --release v1.0.0 --environment production --region us-east-1
   ```

### Troubleshooting

**Deployment Fails with Missing Environment Variables:**
- Run `forklaunch environment validate`
- Use `forklaunch environment sync` to add missing variables
- Fill in required values and retry deployment

**Service Not Detected in Sync:**
- Ensure service directory exists in modules path
- Check that service has proper configuration files
- Run `forklaunch sync service <name>` with verbose output

**Cannot Integrate with Platform:**
- Verify you are logged in: `forklaunch whoami`
- Confirm application ID is correct
- Check that application exists on platform

---

## Quick Reference

### Command Summary

| Command | Purpose | Key Options |
| :------ | :------- | :---------- |
| `sync all` | Sync all projects | `--confirm-all` |
| `sync service` | Sync specific service | - |
| `sync worker` | Sync specific worker | - |
| `sync library` | Sync specific library | - |
| `deploy create` | Create deployment | `--release`, `--environment`, `--region`, `--no-wait` |
| `deploy destroy` | Destroy infrastructure | `--mode` (all/preserve-data) |
| `openapi export` | Export API specs | `--output` |
| `sdk mode` | Change SDK mode | `--type` (generated/live), `--dryrun` |
| `environment validate` | Check env vars | - |
| `environment sync` | Add missing env vars | `--dry-run` |
| `integrate` | Link to platform | `--app` (required) |

### Common Flags

- `--path <PATH>` or `-p`: Specify application root directory
- `--help` or `-h`: Show command help
- `--dry-run` or `-n`: Preview changes without applying
- `--confirm-all` or `-y`: Skip interactive prompts

---

## Additional Resources

- [ForkLaunch CLI Reference](/docs/cli.md)
- [Project Structure Guide](/docs/changing-projects.md)
- [Deployment Guide](/docs/deployment.md)
- [Environment Configuration](/docs/environment-setup.md)
