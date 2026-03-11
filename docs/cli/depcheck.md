---
title: CLI Reference - depcheck
category: References
description: Learn how to use the forklaunch depcheck command.
---

## Overview

Check dependency consistency across your application's projects for version mismatches based on defined project groups.

## Usage

```bash
forklaunch depcheck
```

## Configuration

Dependency checks are configured in `./forklaunch/manifest.toml` using project groups.

### Default Behavior

By default, all projects are checked against each other:

```toml
[project_peer_topology]
myapp = [
    "A",
    "B",
    "C",
]
```

### Custom Groups

To check only specific project combinations, define separate groups:

```toml
[project-peer-topologies]
# A and B will be checked against each other
frontend_group = [
    "A",
    "B",
]

# C will only be checked against itself
backend_group = [
    "C",
]
```

## What It Checks

- **Version mismatches** between projects in the same group
- **Missing dependencies** that should be aligned
- **Inconsistent dependency configurations**

## Output

The command reports:

- Projects with mismatched versions
- Suggested version alignment actions
- Dependency conflicts that need resolution

## Troubleshooting

**Error: "Manifest file not found"**

- Ensure you're in a ForkLaunch application directory
- Check that `./forklaunch/manifest.toml` exists

**Error: "No project groups defined"**

- Add project groups to your manifest.toml
- Use default configuration if checking all projects

**Version mismatch warnings**

- Review conflicting dependencies and their versions
- Update package.json files to align versions
- Run `npm install` after making changes

**Permission errors reading manifest**

- Check file permissions on manifest.toml
- Ensure the forklaunch directory is accessible

**Dependency resolution fails**

- Clear node_modules and package-lock.json
- Run fresh `pnpm/bun install`
- Check for corrupted package cache

## Example Scenarios

1. **Microservices**: Group services that share common dependencies

```toml
[project-peer-topologies]
auth_services = ["auth-api", "user-api"]
data_services = ["analytics-api", "reporting-api"]
```

2. **Frontend/Backend Split**: Separate frontend and backend dependency checks

```toml
[project-peer-topologies]
frontend = ["web-app", "mobile-app"]
backend = ["api-server", "worker"]
```

## Best Practices

- Group projects that should share identical dependency versions
- Keep critical infrastructure services in separate groups
- Use meaningful group names that reflect project relationships

## Related Commands

- [`forklaunch init`](./init) - Initialize projects with proper dependencies
- [`forklaunch eject`](./eject) - Check dependencies after ejection

## Related Documentation

- **[Project Structure](../framework)** - Understanding ForkLaunch project layout
