---
title: CLI Reference - init
category: References
description: Learn how to use the forklaunch init command.
---

# Overview

Create new application resources with ForkLaunch CLI commands. For detailed information, see [Adding Projects](/docs/adding-projects.md).

## Usage

```bash
forklaunch init <COMMAND>
```

## Available Commands

### Initialize Application
```bash
forklaunch init application [OPTIONS] [name]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path` | Project path | _string_ |
| `-o, --modules-path` | Subpath for modules | `src/modules`, `modules` |
| `-d, --database` | Database type | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `-v, --validator` | Schema validator | `zod`, `typebox` |
| `-f, --formatter` | Code formatter | `prettier`, `biome` |
| `-l, --linter` | Linter | `eslint`, `oxlint` |
| `-F, --http-framework` | HTTP framework | `express`, `hyper-express` |
| `-r, --runtime` | Runtime environment | `node`, `bun` |
| `-t, --test-framework` | Testing framework | `vitest`, `jest` |
| `-m, --modules` | Additional modules | `billing-base`, `billing-stripe`, `iam-base`, `iam-better-auth` |
| `-D, --description` | Application description | _string_ |
| `-A, --author` | Application author | _string_ |
| `-L, --license` | License type | `AGPL-3.0`, `GPL-3.0`, `LGPL-3.0`, `Apache-2.0`, `MIT`, `Mozilla-2.0`, `Boost-1.0`, `Unlicense`, `none` |
| `-n, --dryrun` | Preview changes | Flag |

### Initialize Module

```bash
forklaunch init module [OPTIONS] [name]
```

| Option | Description | Values |
| :----- | :---------- | :------ |
| `-p, --path` | Application path | _string_ |
| `-m, --module` | Module to initialize | `billing-base`, `billing-stripe`, `iam-base`, `iam-better-auth` |
| `-d, --database` | Database type | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `-n, --dryrun` | Dry run the command | Flag |

### Initialize Service
```bash
forklaunch init service [OPTIONS] [name]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path` | Application path | _string_ |
| `-d, --database` | Database type | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `-i, --infrastructure` | Infrastructure components (can specify multiple) | `redis`, `s3` |
| `-D, --description` | Service description | _string_ |
| `--mappers` | Generate mapper files for entity/DTO transformation | Flag |
| `-n, --dryrun` | Preview changes | Flag |

### Initialize Worker
```bash
forklaunch init worker [OPTIONS] [name]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path` | Application path | _string_ |
| `-t, --type` | Worker type | `database`, `redis`, `kafka`, `bullmq` |
| `-d, --database` | Database type | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `-D, --description` | Worker description | _string_ |
| `--mappers` | Generate mapper files for entity/DTO transformation | Flag |
| `-n, --dryrun` | Preview changes | Flag |

### Initialize Library
```bash
forklaunch init library [OPTIONS] [name]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path` | Application path | _string_ |
| `-D, --description` | Library description | _string_ |
| `-n, --dryrun` | Preview changes | Flag |

### Initialize Router
```bash
forklaunch init router [OPTIONS] [name]
```

| Option | Description | Values |
| :----- | :---------- | :----- |
| `-p, --path` | Service path (must be in service directory) | _string_ |
| `-i, --infrastructure` | Infrastructure components (can specify multiple) | `redis`, `s3` |
| `-n, --dryrun` | Preview changes | Flag |

## Aliases

Some `init` subcommands have aliases:

- `application`: `app`
- `module`: `mod`
- `service`: `svc`, `project`, `proj`
- `worker`: `wrk`
- `library`: `lib`
- `router`: `controller`, `routes`

## Examples

```bash
# Create a new application with full framework stack
forklaunch init application my-app \
  --path ./my-app \
  --database postgresql \
  --validator zod \
  --http-framework express \
  --runtime node \
  --formatter prettier \
  --linter eslint \
  --test-framework vitest

# Create application with high-performance setup
forklaunch init application my-fast-app \
  --path ./my-fast-app \
  --database postgresql \
  --validator typebox \
  --http-framework hyper-express \
  --runtime bun \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest

# Add a module to existing application
forklaunch init module --path ./my-app --module billing-base

# Add a service with Redis caching
forklaunch init service --path ./my-app/src/modules --database postgresql --infrastructure redis

# Add a worker with BullMQ for advanced queue features
forklaunch init worker --path ./my-app/src/modules --type bullmq

# Add a router with infrastructure support
forklaunch init router --path ./my-app/src/modules/api --infrastructure redis

# Preview changes before applying
forklaunch init service --path ./my-app/src/modules --database postgresql --infrastructure redis --dryrun
```

## Troubleshooting

**Error: "Directory already exists"**
- Choose a different name or remove existing directory
- Use `--dryrun` to preview changes before execution

**Error: "Invalid database type"**
- Check available database options in command help
- Ensure database is supported for the component type

**Error: "Path not found"**
- Verify the specified path exists
- For services/workers, ensure you're in an application directory

**Error: "Module not found"**
- Check available modules with `forklaunch init module --help`
- Verify module name spelling and availability

**Permission denied errors**
- Check file/directory permissions
- Ensure you have write access to the target directory

**Framework package installation issues**
- Ensure you have the correct Node.js/Bun version
- Clear package manager cache and reinstall
- Check for conflicting global package installations

**Infrastructure setup problems**
- Verify Redis/S3 credentials are properly configured
- Check Docker containers are running for local development
- Review environment variable configuration

## Framework Best Practices

### Choosing the Right Stack
- **Express + Node.js**: Best for traditional web applications with extensive middleware ecosystem
- **Hyper-Express + Bun**: Ideal for high-performance applications requiring maximum speed
- **TypeBox**: Better for compile-time type safety and smaller bundle sizes
- **Zod**: Excellent for runtime validation and complex schema transformations

### Infrastructure Planning
- **Redis**: Essential for session management, caching, and real-time features
- **S3**: Required for file uploads, media storage, and static asset hosting
- **Database**: Choose based on your data complexity and scaling requirements

### Development Workflow
1. Start with `--dryrun` to preview your project structure
2. Use consistent framework choices across all services
3. Leverage the universal SDK for service-to-service communication
4. Choose infrastructure conifguration early to avoid migration issues


## Related Commands

- [`forklaunch add`](../adding-projects) - Add components to existing projects
- [`forklaunch depcheck`](./depcheck) - Check dependencies after initialization

## Related Documentation

- **[Adding Projects Guide](../adding-projects)** - Comprehensive project creation guide
