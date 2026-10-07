---
title: Delete Command
category: CLI Reference
description: CLI reference for ForkLaunch delete commands.
---

## Overview

Delete ForkLaunch project components with confirmation prompts.

**⚠️ Warning**: Deletion operations are irreversible. Always ensure you have backups before proceeding.

## Usage

### Service

```bash
forklaunch delete service <service-name> [OPTIONS]
```

| Option              | Description                   | Values                                |
| :------------------ | :---------------------------- | :------------------------------------ |
| `<service-name>`    | Name of the service to delete | _string_                              |
| `-p, --path <path>` | Application path              | _string_ (default: current directory) |
| `-c, --continue`    | Skip confirmation prompt      | None                                  |
| `-h, --help`        | Show help                     | None                                  |

**Examples:**

```bash
# Delete a service with confirmation
forklaunch delete service billing

# Delete without confirmation
forklaunch delete service billing --continue

# Delete in specific directory
forklaunch delete service billing --path ~/my-app
```

### Worker

Delete a worker and its queue configuration.

```bash
forklaunch delete worker <worker-name> [OPTIONS]
```

| Option              | Description                  | Values                                |
| :------------------ | :--------------------------- | :------------------------------------ |
| `<worker-name>`     | Name of the worker to delete | _string_                              |
| `-p, --path <path>` | Application path             | _string_ (default: current directory) |
| `-c, --continue`    | Skip confirmation prompt     | None                                  |
| `-h, --help`        | Show help                    | None                                  |

**Examples:**

```bash
# Delete a worker
forklaunch delete worker email-processor

# Delete multiple workers
forklaunch delete worker email-processor --continue
forklaunch delete worker sms-processor --continue
```

### Router

Delete a router and its route definitions.

```bash
forklaunch delete router <router-name> [OPTIONS]
```

| Option              | Description                  | Values                                |
| :------------------ | :--------------------------- | :------------------------------------ |
| `<router-name>`     | Name of the router to delete | _string_                              |
| `-p, --path <path>` | Application path             | _string_ (default: current directory) |
| `-c, --continue`    | Skip confirmation prompt     | None                                  |
| `-h, --help`        | Show help                    | None                                  |

**Examples:**

```bash
# Delete a router
forklaunch delete router api-v1

# Delete with custom path
forklaunch delete router admin --path ./microservice
```

### Library

Delete a library and its exports.

```bash
forklaunch delete library <library-name> [OPTIONS]
```

| Option              | Description                   | Values                                |
| :------------------ | :---------------------------- | :------------------------------------ |
| `<library-name>`    | Name of the library to delete | _string_                              |
| `-p, --path <path>` | Application path              | _string_ (default: current directory) |
| `-c, --continue`    | Skip confirmation prompt      | None                                  |
| `-h, --help`        | Show help                     | None                                  |

**Examples:**

```bash
# Delete a library
forklaunch delete library utils

# Delete shared library
forklaunch delete library validation --continue
```

## Global Options

| Option              | Description                                   |
| :------------------ | :-------------------------------------------- |
| `-p, --path <path>` | Application path (default: current directory) |
| `-c, --continue`    | Skip confirmation prompt                      |
| `-h, --help`        | Show help                                     |

## Examples

```bash
# Delete with confirmation
forklaunch delete service billing

# Delete without confirmation
forklaunch delete service billing --continue

# Delete in specific directory
forklaunch delete service billing --path ~/my-app

# Batch deletion script
forklaunch delete service old-service --continue
forklaunch delete worker deprecated-worker --continue
```

## Safety Features

- **Confirmation prompts** by default
- **File conflict detection**
- **Dependency cleanup**
- Use `--continue` to skip prompts for automation

## Troubleshooting

**Error: Component not found**

- Verify component name and path
- Check current directory is correct application

**Error: Dependency conflicts**

- Review dependent services before deletion
- Use `forklaunch depcheck` after deletion

**Error: Permission denied**

- Check file permissions
- Ensure no processes are using component files

## Related Commands

- [`forklaunch add`](/docs/adding-projects.md) - Add components
- [`forklaunch change`](/docs/changing-projects.md) - Modify components
- [`forklaunch depcheck`](/docs/cli/depcheck.md) - Check dependencies

## Related Documentation

- **[Deleting Projects Guide](../deleting-projects)** - Comprehensive deletion guide
