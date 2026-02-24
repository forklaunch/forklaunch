---
title: CLI Reference
category: References
description: Reference for the ForkLaunch CLI commands.
---

## Overview

The ForkLaunch CLI provides commands for managing your application development.

## Project Management Commands

| Command | Description | Alias |
| :------ | :---------- | :---- |
| `forklaunch init` | Initialize a new ForkLaunch project | - |
| `forklaunch add` | Add new components to existing project | - |
| `forklaunch change` | Modify existing project components | - |
| `forklaunch delete` | Delete project components | `del` |
| `forklaunch sync` | Sync existing project components with artifacts | - |

## Development Commands

| Command | Description | Alias |
| :------ | :---------- | :---- |
| `forklaunch depcheck` | Check dependency alignment across projects | - |
| `forklaunch eject` | Eject dependencies from ForkLaunch management | - |
| `forklaunch config` | Manage application configuration | - |

## Platform Commands

| Command | Description | Alias |
| :------ | :---------- | :---- |
| `forklaunch login` | Login to ForkLaunch platform | - |
| `forklaunch logout` | Log out of ForkLaunch platform | - |
| `forklaunch whoami` | Show current logged in user | - |

## Utility Commands

| Command | Description | Alias |
| :------ | :---------- | :---- |
| `forklaunch version` | Show CLI version | - |
| `forklaunch help` | Show help information | - |

## Global Options

| Option | Description |
| :----- | :---------- |
| `-h, --help` | Show help |
| `-V, --version` | Show version |

## Detailed Documentation

For detailed information about specific commands:

### Project Management
- [init](/docs/cli/init.md) - Project initialization
- [Adding Projects](/docs/adding-projects.md) - Add components to projects, sync existing or new components
- [Changing Projects](/docs/changing-projects.md) - Modify existing components
- [delete](/docs/cli/delete.md) - Delete project components
- [Deleting Projects](/docs/deleting-projects.md) - Comprehensive deletion guide, clean up project artifacts

### Development Tools
- [depcheck](/docs/cli/depcheck.md) - Dependency management
- [eject](/docs/cli/eject.md) - Dependency ejection
- [config](/docs/cli/config.md) - Configuration options

### Authentication & Platform
- [authentication](/docs/cli/authentication.md) - Login, logout, and user management

## Quickstart Guide

### Common Workflows

**Create New Project:**
```bash
forklaunch init application my-app --path ./my-app --database postgresql --runtime node
```

**Add Components:**
You can use `forklaunch init` or `forklaunch add` to add projects to your application:
```bash
forklaunch add service billing --database postgresql
forklaunch add worker email-processor --type bullmq
forklaunch add router api-v1
forklaunch add library utils
```

**Modify Components:**
```bash
forklaunch change service billing --database mysql
forklaunch change worker email-processor --type kafka
forklaunch change router api-v1 --new-name api-v2
forklaunch change library utils --description "Shared utilities"
```

**Remove Components:**
```bash
forklaunch delete service old-billing
forklaunch delete worker deprecated-processor
forklaunch delete router legacy-api
forklaunch delete library unused-utils
```

**Sync Components:**
```bash
forklaunch sync all
forklaunch sync service new-billing
forklaunch sync worker custom-processer
forklaunch sync library new-utils
```

**Development Utilities:**
```bash
forklaunch depcheck                    # Check dependencies
forklaunch eject                       # Eject from ForkLaunch
forklaunch config --show               # Show configuration
```

**Platform Integration:**
```bash
forklaunch login                       # Login to platform
forklaunch whoami                      # Check current user
forklaunch logout                      # Logout
```
