---
title: Eject Command
category: CLI
description: Learn how to eject ForkLaunch dependencies to customize them.
---

## Overview

Copy ForkLaunch dependencies into your project for customization.

## Usage

```bash
forklaunch eject [options]
```

## Options

| Option           | Short | Description                                       | Default               |
| ---------------- | ----- | ------------------------------------------------- | --------------------- |
| `--continue`     | `-c`  | Continue the eject operation without confirmation | `false`               |
| `--dependencies` | `-d`  | Specify which dependencies to eject               | Interactive selection |
| `--dryrun`       | `-n`  | Show what would be ejected without making changes | `false`               |
| `--base_path`    | `-b`  | Path to the application directory                 | Current directory     |

## Interactive Mode

When run without specifying dependencies, the command will:

1. **Scan for ejectable dependencies** - Only ForkLaunch packages can be ejected
2. **Show available options** - Display a list of ejectable dependencies
3. **Prompt for selection** - Allow you to choose which dependencies to eject
4. **Collision detection** - Check for file conflicts before proceeding
5. **Confirmation** - Ask for final confirmation (unless `--continue` is used)

## What Happens During Ejection

### 1. File Copying

- Source code is copied from `node_modules/@forklaunch/*` to your project
- Files maintain their directory structure
- Package metadata is preserved

### 2. Dependency Updates

- Ejected packages are removed from `package.json` dependencies
- Internal imports are updated to point to local files
- Index files are intelligently merged when conflicts exist

### 3. Import Path Rewriting

- Automatically updates import statements throughout your codebase
- Replaces package imports with relative file paths
- Handles nested dependencies correctly

## Examples

```bash
# Interactive selection
forklaunch eject

# Eject specific dependencies
forklaunch eject -d @forklaunch/service-billing -d @forklaunch/service-iam

# Preview changes
forklaunch eject --dryrun -d @forklaunch/service-iam

# Skip confirmation
forklaunch eject --continue -d @forklaunch/service-billing
```

## File Collision Handling

- **Index.ts files**: Merged automatically, preserves existing exports
- **Other files**: Operation stops if conflicts detected

## Post-Ejection Notes

⚠️ **Important**:

- Ejected code becomes your responsibility to maintain
- No automatic updates from ForkLaunch
- Test thoroughly after ejection

## Troubleshooting

**Error: "File collisions detected"**

- Review listed conflicting files
- Remove or rename existing files before ejecting

**Error: "Dependency not ejectable"**

- Only `@forklaunch/*` packages can be ejected
- Verify package name is correct

**Missing dependencies after ejection**

- Check `package.json` for missing transitive dependencies
- Install any missing packages manually

## Related Commands

- [`forklaunch depcheck`](./depcheck) - Check dependency alignment
- [`forklaunch change`](../changing-projects) - Modify ejected services

## Related Documentation

- **[Framework Documentation](../framework)** - Framework overview and features
- **[Customization Guide](../customization)** - Project customization options
