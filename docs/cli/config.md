---
title: CLI Reference - config
category: References
description: Learn how to use the forklaunch config command.
---

## Overview

The `config` command pulls and pushes environment configuration between your local `.env` files and the ForkLaunch platform. You must be authenticated to use this command.

## Usage

```bash
forklaunch config [COMMAND]
```

### Available Commands

| Command | Description |
| :------ | :---------- |
| `pull`  | Pull environment variables from the platform to a local `.env` file |
| `push`  | Push a local `.env` file to the platform |

---

### `config pull`

```bash
forklaunch config pull -a <APP_ID> -r <REGION> -e <ENV> [-s <SERVICE>] [-o <FILE>]
```

| Flag | Short | Required | Description |
| :--- | :---- | :------- | :---------- |
| `--app` | `-a` | Yes | Application ID |
| `--region` | `-r` | Yes | Region (e.g. `us-east-1`) |
| `--environment` | `-e` | Yes | Environment name (e.g. `production`, `staging`) |
| `--service` | `-s` | No | Filter to a specific service or worker name |
| `--output` | `-o` | No | Output file path (defaults to `<environment>.env`) |

#### Examples

```bash
# Pull production config to production.env
forklaunch config pull -a app-123 -r us-east-1 -e production

# Pull staging config for a specific service
forklaunch config pull -a app-123 -r us-east-1 -e staging -s billing-service

# Pull to a custom file path
forklaunch config pull -a app-123 -r us-east-1 -e production -o ./config/.env.prod
```

---

### `config push`

```bash
forklaunch config push -a <APP_ID> -r <REGION> -e <ENV> [-i <FILE>]
```

| Flag | Short | Required | Description |
| :--- | :---- | :------- | :---------- |
| `--app` | `-a` | Yes | Application ID |
| `--region` | `-r` | Yes | Region (e.g. `us-east-1`) |
| `--environment` | `-e` | Yes | Environment name (e.g. `production`, `staging`) |
| `--input` | `-i` | No | Input file path (defaults to `<environment>.env`) |

#### Examples

```bash
# Push from production.env
forklaunch config push -a app-123 -r us-east-1 -e production

# Push from a custom file path
forklaunch config push -a app-123 -r us-east-1 -e production -i ./config/.env.prod
```

---

## Environment File Format

The `.env` file uses comment headers to separate variables by source. Application-level variables appear under `# application`, while service- and worker-scoped variables appear under headers with the component name and ID.

```env
# application
DATABASE_URL=postgres://...
REDIS_URL=redis://...

# billing-service (svc-id-123)
STRIPE_KEY=sk_test_...
WEBHOOK_SECRET=whsec_...

# email-worker (wkr-id-456)
SMTP_HOST=smtp.example.com
```

When pushing, the comment headers determine which service or worker each variable belongs to. The `(id)` portion is used to resolve the target entity.

---

## Troubleshooting

**Error: "Authentication required"**

- Run `forklaunch login` to authenticate
- Check session status with `forklaunch whoami`

**Error: "Application not found"**

- Verify the application ID is correct
- Ensure you have access to the application in your organization

**Error: "Environment not found"**

- Verify the environment name (e.g. `production`, `staging`, `development`)
- Check that the environment exists for the given application

**Error: "Failed to pull/push config"**

- Check internet connectivity
- Ensure the platform API is reachable
- Verify your authentication token hasn't expired

**Error: "File not found" (push)**

- Verify the input file path exists
- Check file permissions and accessibility

## Related Commands

- [`forklaunch login`](./authentication.md) - Authenticate with platform
- [`forklaunch whoami`](./authentication.md) - Check authentication status

## Related Documentation

- **[Authentication Guide](./authentication.md)** - Platform authentication
