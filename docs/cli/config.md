---
title: CLI Reference - config
category: References
description: Learn how to use the forklaunch config command.
---

## Overview

THIS COMMAND IS CURRENTLY UNDER DEVELOPMENT AND IS NOT YET AVAILABLE.

The `config` command manages application configuration between your local environment and the ForkLaunch platform. You must be authenticated to use this command.

## Usage

```bash
forklaunch config [COMMAND]
```

### Available Commands

| Command | Description                          |
| :------ | :----------------------------------- |
| `pull`  | Pull environment configuration from platform |
| `push`  | Push environment configuration to platform   |

### pull

```bash
forklaunch config pull --region <region> --environment <env> [options]
```

**Required:**
- `-r, --region <region>` - Region (e.g., `us-east-1`)
- `-e, --environment <env>` - Environment name (e.g., `production`, `staging`)

**Optional:**
- `-s, --service <name>` - Filter to a specific service name
- `-o, --output <file>` - Output file path (defaults to `<environment>.env`)
- `-p, --path <path>` - Path to application root

### push

```bash
forklaunch config push --region <region> --environment <env> [options]
```

**Required:**
- `-r, --region <region>` - Region (e.g., `us-east-1`)
- `-e, --environment <env>` - Environment name (e.g., `production`, `staging`)

**Optional:**
- `-i, --input <file>` - Input file path (defaults to `<environment>.env`)
- `-p, --path <path>` - Path to application root

### Examples

```bash
# Pull configuration for staging
forklaunch config pull --region us-east-1 --environment staging

# Pull configuration for a specific service
forklaunch config pull --region us-east-1 --environment staging --service payments

# Pull configuration to a specific file
forklaunch config pull --region us-east-1 --environment production --output ./config/.env.prod

# Push configuration for staging
forklaunch config push --region us-east-1 --environment staging

# Push configuration from a specific file
forklaunch config push --region us-east-1 --environment production --input ./config/.env.prod
```

## Troubleshooting

**Error: "Authentication required"**

- Run `forklaunch login` to authenticate
- Check session status with `forklaunch whoami`

**Error: "Permission denied"**

- Ensure you have access to the configuration
- Contact your organization admin if using team configurations

**Error: "File not found" (push)**

- Verify the input file path exists
- Check file permissions and accessibility

## Related Commands

- [`forklaunch login`](./authentication) - Authenticate with platform
- [`forklaunch whoami`](./authentication) - Check authentication status

## Related Documentation

- **[Authentication Guide](./authentication)** - Platform authentication
