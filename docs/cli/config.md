---
title: CLI Reference - config
category: References
description: Learn how to use the forklaunch config command.
---

## Overview

The `config` command manages application configuration between your local environment and the ForkLaunch platform. You must be authenticated to use this command.

`pull` and `push` move a whole `.env` file at a time; `set` and `unset` act on one
variable.

## Usage

```bash
forklaunch config [COMMAND]
```

### Available Commands

| Command | Description                          |
| :------ | :----------------------------------- |
| `pull`  | Pull environment configuration from platform |
| `push`  | Push environment configuration to platform   |
| `set`   | Set a single variable without touching the rest of the scope |
| `unset` | Mark a variable as deliberately absent so the deploy gate stops requiring it |

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

> Push is authoritative per scope. Any variable that exists on the platform in a
> scope the file touches, but is missing from the file, is marked unset **and its
> stored value is erased**. Use `config unset` to retire a single variable.

### set

```bash
forklaunch config set KEY=VALUE --region <region> --environment <env> [options]
```

Sets one variable, leaving every other variable in the scope alone.

**Required:**
- `KEY=VALUE` - The variable to set (quote values containing spaces)
- `-r, --region <region>` - Region (e.g., `us-east-1`)
- `-e, --environment <env>` - Environment name (e.g., `production`, `staging`)

**Optional:**
- `-s, --service <name>` - Scope to a service or worker (defaults to application scope)
- `-f, --force` - Set the variable even when no component declares it
- `-p, --path <path>` - Path to application root

The output distinguishes `ADDED` (the name did not exist in this scope) from
`UPDATED` (an existing value was replaced). If the name is one that no component
declares and that the platform has never seen, `set` warns, suggests the closest
declared names, and asks for confirmation before writing:

```
[WARN] 'TWILIO_ACCOUNT_TOKEN' is not declared by any component in this application, and no variable by that name exists in production (us-east-1).
[WARN]        Did you mean 'TWILIO_ACCOUNT_SID' or 'TWILIO_AUTH_TOKEN'?
[WARN]        Setting it will store the value under a name nothing reads.
? Set 'TWILIO_ACCOUNT_TOKEN' anyway? (y/N)
```

The declared names come from the config the platform returns plus a scan of the
local workspace. Neither is a complete picture on its own, so this is a warning
rather than a refusal — a scripted run prints the warning and proceeds, and
`--force` skips the prompt.

### unset

```bash
forklaunch config unset KEY --region <region> --environment <env> [options]
```

Marks a variable as deliberately absent. The deploy gate stops requiring a value
for it. Use this for variables that are injected at runtime and must never hold a
value — `ECS_AGENT_URI`, which the ECS agent supplies per task, is the canonical
case.

**Required:**
- `KEY` - Name of the variable to mark unset
- `-r, --region <region>` - Region (e.g., `us-east-1`)
- `-e, --environment <env>` - Environment name (e.g., `production`, `staging`)

**Optional:**
- `-s, --service <name>` - Scope to a service or worker (defaults to application scope)
- `-y, --yes` - Skip the confirmation prompt (for CI/scripted use)
- `-p, --path <path>` - Path to application root

Unsetting a variable that currently holds a value **destroys that value** — the
platform stores an empty string, and it cannot be recovered from the CLI or the
dashboard. When that is the case, `unset` says so and asks for confirmation.
Without a terminal to ask on, it refuses unless `--yes` is given. A variable that
holds no value is unset with no prompt.

Unlike `config push`, this touches only the key you name.

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

# Set one variable at application scope
forklaunch config set STRIPE_API_KEY=sk_live_xxx --region us-east-1 --environment production

# Set one variable on a single service
forklaunch config set QUEUE_NAME=orders --region us-east-1 --environment production --service payments

# Mark a runtime-injected variable as deliberately absent
forklaunch config unset ECS_AGENT_URI --region us-east-1 --environment production
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
