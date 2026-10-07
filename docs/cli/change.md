---
title: Change Command
category: CLI Reference
description: CLI reference for ForkLaunch change commands.
---

## Overview

Modify the configuration of existing ForkLaunch project components. Use `--dryrun` to preview changes before applying them.

## Usage

```bash
forklaunch change <COMMAND>
```

**Aliases:** `modify`, `alter`

## Available Commands

### Application

Change application-level configuration such as runtime, framework, tooling, and metadata.

```bash
forklaunch change application [OPTIONS]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path <path>` | Application root path | _string_ (default: current directory) |
| `-N <name>` | Rename the application | _string_ |
| `-v, --validator <validator>` | Schema validator | `zod`, `typebox` |
| `-f, --formatter <formatter>` | Code formatter | `prettier`, `biome` |
| `-l, --linter <linter>` | Linter | `eslint`, `oxlint` |
| `-F, --http-framework <framework>` | HTTP framework | `express`, `hyper-express` |
| `-r, --runtime <runtime>` | Runtime environment | `node`, `bun` |
| `-t, --test-framework <framework>` | Testing framework | `vitest`, `jest` |
| `-D, --description <desc>` | Application description | _string_ |
| `-A, --author <author>` | Application author | _string_ |
| `-L, --license <license>` | License type | `AGPL-3.0`, `GPL-3.0`, `LGPL-3.0`, `Apache-2.0`, `MIT`, `Mozilla-2.0`, `Boost-1.0`, `Unlicense`, `none` |
| `-n, --dryrun` | Preview changes without applying them | Flag |
| `-c, --confirm` | Skip confirmation prompts | Flag |

**Aliases:** `app`

**Examples:**

```bash
# Switch runtime to Bun (preview first)
forklaunch change application --runtime bun --dryrun
forklaunch change application --runtime bun

# Switch HTTP framework
forklaunch change application --http-framework hyper-express --dryrun
forklaunch change application --http-framework hyper-express

# Update multiple tooling options at once
forklaunch change application --formatter biome --linter oxlint --dryrun
forklaunch change application --formatter biome --linter oxlint

# Change in specific directory
forklaunch change application --path ~/my-app --runtime bun --dryrun
forklaunch change application --path ~/my-app --runtime bun
```

---

### Service

Change service configuration, infrastructure, database, or convert a service to a worker.

```bash
forklaunch change service [OPTIONS]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path <path>` | Service path | _string_ (default: current directory) |
| `-N <name>` | Rename the service | _string_ |
| `-d, --database <type>` | Database type | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `-i, --infrastructure <infra>` | Infrastructure components (can specify multiple) | `redis`, `s3` |
| `-D, --description <desc>` | Service description | _string_ |
| `--to <type>` | Convert service to another project type | `worker` |
| `-t, --type <type>` | Worker type (required when `--to worker`) | `database`, `redis`, `kafka`, `bullmq` |
| `-n, --dryrun` | Preview changes without applying them | Flag |
| `-c, --confirm` | Skip confirmation prompts | Flag |

**Aliases:** `svc`

**Examples:**

```bash
# Change database
forklaunch change service --path ./my-app/src/modules/payments --database mongodb --dryrun
forklaunch change service --path ./my-app/src/modules/payments --database mongodb

# Add Redis infrastructure
forklaunch change service --path ./my-app/src/modules/users --infrastructure redis --dryrun
forklaunch change service --path ./my-app/src/modules/users --infrastructure redis

# Convert service to a BullMQ worker
forklaunch change service --path ./my-app/src/modules/email --to worker --type bullmq --dryrun
forklaunch change service --path ./my-app/src/modules/email --to worker --type bullmq
```

---

### Worker

Change worker configuration, type, or convert a worker to a service.

```bash
forklaunch change worker [OPTIONS]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path <path>` | Worker path | _string_ (default: current directory) |
| `-N <name>` | Rename the worker | _string_ |
| `-t, --type <type>` | Worker queue type | `database`, `redis`, `kafka`, `bullmq` |
| `-d, --database <type>` | Database type | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `-D, --description <desc>` | Worker description | _string_ |
| `--to <type>` | Convert worker to another project type | `service` |
| `-n, --dryrun` | Preview changes without applying them | Flag |
| `-c, --confirm` | Skip confirmation prompts | Flag |

**Aliases:** `wrk`

**Examples:**

```bash
# Change worker queue type
forklaunch change worker --path ./my-app/src/modules/email-processor --type kafka --dryrun
forklaunch change worker --path ./my-app/src/modules/email-processor --type kafka

# Convert worker to a service
forklaunch change worker --path ./my-app/src/modules/email-processor --to service --dryrun
forklaunch change worker --path ./my-app/src/modules/email-processor --to service
```

---

### Router

Rename a router or add mapper files to an existing router.

```bash
forklaunch change router [OPTIONS]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path <path>` | Service path (must be in a service directory) | _string_ (default: current directory) |
| `-e <existing-name>` | Original name of the router | _string_ |
| `-N <new-name>` | New name for the router | _string_ |
| `--add-mappers` | Generate mapper files from existing schemas and entities | Flag |
| `-n, --dryrun` | Preview changes without applying them | Flag |
| `-c, --confirm` | Skip confirmation prompts | Flag |

**Aliases:** `controller`, `routes`

**Examples:**

```bash
# Rename a router
forklaunch change router --path ./my-app/src/modules/payments -e charges -N transactions --dryrun
forklaunch change router --path ./my-app/src/modules/payments -e charges -N transactions

# Add mappers to an existing router
forklaunch change router --path ./my-app/src/modules/payments --add-mappers --dryrun
forklaunch change router --path ./my-app/src/modules/payments --add-mappers
```

---

### Library

Rename or update the description of an existing library.

```bash
forklaunch change library [OPTIONS]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path <path>` | Library path | _string_ (default: current directory) |
| `-N <name>` | Rename the library | _string_ |
| `-D, --description <desc>` | Library description | _string_ |
| `-n, --dryrun` | Preview changes without applying them | Flag |
| `-c, --confirm` | Skip confirmation prompts | Flag |

**Aliases:** `lib`

**Examples:**

```bash
# Rename a library
forklaunch change library --path ./my-app/src/modules/utils -N shared-utils --dryrun
forklaunch change library --path ./my-app/src/modules/utils -N shared-utils

# Update description
forklaunch change library --path ./my-app/src/modules/utils --description "Shared utility functions"
```

---

## Global Options

| Option | Description |
| :----- | :---------- |
| `-p, --path <path>` | Path to the component (default: current directory) |
| `-n, --dryrun` | Preview changes without applying them |
| `-c, --confirm` | Skip confirmation prompts |
| `-h, --help` | Show help |

## Aliases

Some `change` subcommands have aliases:

- `application`: `app`
- `service`: `svc`
- `worker`: `wrk`
- `library`: `lib`
- `router`: `controller`, `routes`

The top-level `change` command also has aliases: `modify`, `alter`.

## Safe Change Workflow

Always use `--dryrun` before applying changes to preview what will be modified:

```bash
# 1. Commit your current state
git add . && git commit -m "Before change"

# 2. Preview the change
forklaunch change application --runtime bun --dryrun

# 3. Apply the change
forklaunch change application --runtime bun

# 4. Install dependencies and sync
pnpm install
forklaunch sync all

# 5. Test locally
pnpm dev

# 6. Commit
git add . && git commit -m "Changed to Bun runtime"
```

## Troubleshooting

**Error: Component not found**

- Verify the path is inside an application directory
- Check that the component name matches what's in `.forklaunch/manifest.toml`

**Error: Invalid value for option**

- Check available values with `forklaunch change <subcommand> --help`
- Ensure option values are spelled correctly (e.g., `postgresql` not `postgres`)

**Error: `--type` required when using `--to worker`**

- When converting a service to a worker with `--to worker`, you must also supply `-t / --type`

**Permission denied errors**

- Check file/directory permissions
- Ensure no processes are actively using the component files

## Related Commands

- [`forklaunch init`](./init) - Create new components
- [`forklaunch delete`](./delete) - Remove components
- [`forklaunch sync`](./sync) - Apply changes to generated artifacts

## Related Documentation

- **[Adding Projects Guide](../adding-projects)** - Full guide to creating and configuring projects
