---
title: Release Command
category: CLI Reference
description: Create and manage application releases for deployment.
---

## Overview

The `release` command packages your application code, analyzes dependencies, and creates a versioned release artifact on the ForkLaunch platform. Releases are immutable snapshots that can be deployed to multiple environments.

## Usage

```bash
forklaunch release create --version <version> [options]
```

## Prerequisites

Before creating releases, you must:

1. **Login to Platform**
   ```bash
   forklaunch login
   ```

2. **Integrate with Platform Application**
   ```bash
   forklaunch integrate --app <application-id>
   ```

## Arguments

**Required:**
- `-v, --version <version>` - Semantic version number (e.g., `1.0.0`, `2.1.3-beta`)

**Optional:**
- `-n, --notes <notes>` - Release notes or description
- `-p, --path <path>` - Path to application root (defaults to current directory)
- `--dry-run` - Simulate release without creating it
- `--local` - Package local code and upload to S3 (skip mode selection prompt)
- `--git` - Use git-based release flow (skip mode selection prompt)
- `--skip-sync` - Skip automatic sync of projects with manifest before creating release

## Release Mode Selection

When neither `--local` nor `--git` is specified, the CLI presents an interactive prompt:

```bash
$ forklaunch release create --version 1.0.0

? How would you like to release?
> Package locally (upload code directly)
  Use git (connect GitHub repository)
```

Local mode is the default. Use `--local` or `--git` to bypass the prompt.

## Creating a Release

### Basic Release

```bash
$ forklaunch release create --version 1.0.0

[INFO] Creating release 1.0.0...
[INFO] Analyzing project structure...
[INFO] Found 3 services, 2 workers, 1 router, 1 library

[INFO] Analyzing dependencies...
[OK] Scanning package.json dependencies
[OK] Validating workspace configuration

[INFO] Scanning environment variables...
[OK] Found 15 environment variables across 5 components

[INFO] Exporting OpenAPI specifications...
[OK] Exported users.json
[OK] Exported payments.json
[OK] Exported notifications.json

[INFO] Packaging source code...
[OK] Source uploaded to S3

[INFO] Creating release manifest...
[OK] Release manifest generated

[INFO] Uploading to platform...
[OK] Release 1.0.0 created successfully!

[INFO] Release ID: rel_xyz789abc
[INFO] Release URL: https://platform.forklaunch.com/releases/rel_xyz789abc

[INFO] You can now deploy:
  forklaunch deploy create --release 1.0.0 --environment staging --region us-west-2
```

### Release with Notes

```bash
$ forklaunch release create \
  --version 1.2.0 \
  --notes "Added payment retry logic and improved error handling"

[INFO] Creating release 1.2.0...
[INFO] Release notes: Added payment retry logic and improved error handling
[...]
[OK] Release 1.2.0 created successfully!
```

### Dry Run

Preview what would be released without creating it:

```bash
$ forklaunch release create --version 1.0.0 --dry-run

[DRY RUN] Would create release 1.0.0 with:

Services:
  - users
  - payments
  - notifications

Workers:
  - email-processor
  - data-export

Environment Variables:
  - DATABASE_URL (application-level)
  - REDIS_URL (application-level)
  - STRIPE_SECRET_KEY (service: payments)
  - SENDGRID_API_KEY (worker: email-processor)
  ... 11 more

[DRY RUN] No release created
```

### Local Package Mode

Package and upload code directly without GitHub integration:

```bash
$ forklaunch release create --version 1.0.0 --local

[INFO] Creating release 1.0.0 (local mode)...
[INFO] Packaging local directory...
[OK] Source code packaged (125MB)
[OK] Uploaded to S3
[...]
[OK] Release 1.0.0 created successfully!
```

### Git Mode

Use git-based release flow connecting to your repository:

```bash
$ forklaunch release create --version 1.0.0 --git

[INFO] Creating release 1.0.0 (git mode)...
[INFO] Git commit: abc123def456
[INFO] Git branch: main
[OK] Source uploaded via GitHub integration
[...]
[OK] Release 1.0.0 created successfully!
```

## Automatic Sync

Before creating a release, the CLI automatically runs `sync all` to ensure all projects are synchronized with the manifest. Use `--skip-sync` to bypass this step.

## What Gets Packaged

### Source Code

- All services in `src/modules/*/`
- All workers in `src/modules/*/`
- All libraries in `src/modules/*/`
- Shared configuration files
- Package dependency manifests

### Dependencies

The release includes your application's package dependencies from `package.json` and workspace configuration.

### Environment Variables

Scanned from:
- `.env` files
- `process.env` usage in code
- Service configurations
- Worker configurations

### API Specifications

- OpenAPI specs for all services
- Internal SDK definitions

### Release Manifest

Complete metadata including:
- Project structure
- Dependency graph
- Environment variable requirements
- Build configuration
- Runtime requirements

## Release Versioning

### Semantic Versioning

ForkLaunch follows semantic versioning (semver):

```
MAJOR.MINOR.PATCH
```

- **MAJOR**: Breaking changes
- **MINOR**: New features (backwards compatible)
- **PATCH**: Bug fixes

**Examples:**
```bash
# Major version - breaking changes
forklaunch release create --version 2.0.0

# Minor version - new features
forklaunch release create --version 1.1.0

# Patch version - bug fixes
forklaunch release create --version 1.0.1
```

### Pre-release Versions

```bash
# Beta releases
forklaunch release create --version 1.0.0-beta.1
forklaunch release create --version 1.0.0-beta.2

# Alpha releases
forklaunch release create --version 1.0.0-alpha.1

# Release candidates
forklaunch release create --version 1.0.0-rc.1
```

## CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/release.yml
name: Create Release

on:
  push:
    tags:
      - 'v*'

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2

      - name: Setup Node
        uses: actions/setup-node@v2
        with:
          node-version: '20'

      - name: Install ForkLaunch CLI
        run: npm install -g forklaunch

      - name: Login to Platform
        run: forklaunch login --token "${{ secrets.FORKLAUNCH_TOKEN }}"

      - name: Extract version from tag
        id: version
        run: echo "VERSION=${GITHUB_REF#refs/tags/v}" >> $GITHUB_OUTPUT

      - name: Create Release
        run: |
          forklaunch release create \
            --version ${{ steps.version.outputs.VERSION }} \
            --local \
            --notes "${{ github.event.head_commit.message }}"

      - name: Deploy to Staging
        run: |
          forklaunch deploy create \
            --release ${{ steps.version.outputs.VERSION }} \
            --environment staging \
            --region us-west-2
```

## Common Workflows

### Development Workflow

```bash
# 1. Develop features
git checkout -b feature/payment-retry
# ... make changes ...
git commit -m "Add payment retry logic"
git push origin feature/payment-retry

# 2. Merge to main
git checkout main
git merge feature/payment-retry

# 3. Create release
forklaunch release create --version 1.1.0 --notes "Added payment retry logic"

# 4. Deploy to staging
forklaunch deploy create --release 1.1.0 --environment staging --region us-west-2

# 5. Test in staging
# ... test the deployment ...

# 6. Deploy to production
forklaunch deploy create --release 1.1.0 --environment production --region us-west-2
```

### Hotfix Workflow

```bash
# 1. Create hotfix branch
git checkout -b hotfix/critical-bug main

# 2. Fix the bug
# ... make changes ...
git commit -m "Fix critical payment bug"

# 3. Create patch release
forklaunch release create --version 1.0.1 --notes "Emergency fix for payment bug"

# 4. Deploy immediately
forklaunch deploy create --release 1.0.1 --environment production --region us-west-2

# 5. Merge back to main
git checkout main
git merge hotfix/critical-bug
```

## Best Practices

1. **Commit Before Release**: Always commit changes before creating releases
2. **Semantic Versioning**: Follow semver strictly for predictable updates
3. **Meaningful Notes**: Add clear release notes describing changes
4. **Test Before Production**: Deploy to staging first, test thoroughly
5. **Immutable Releases**: Never modify a release after creation
6. **Version Control**: Tag Git commits matching release versions
7. **Automate**: Use CI/CD to automate release creation
8. **Document Changes**: Maintain CHANGELOG.md alongside releases

## Troubleshooting

### Not Integrated with Platform

```bash
[ERROR] Application not integrated with platform
[INFO] Run: forklaunch integrate --app <application-id>
```

**Solution**: Run `forklaunch integrate` first.

### Version Already Exists

```bash
[ERROR] Release 1.0.0 already exists
```

**Solution**: Use a new version number.

### Missing Dependencies

```bash
[WARNING] Could not determine database type for service: users
```

**Solution**: Ensure dependencies are properly defined in code.

### Source Code Too Large

```bash
[ERROR] Source code package exceeds 500MB limit
```

**Solution**: Check `.gitignore` includes `node_modules`, `.next`, etc.

## Related Commands

- [`forklaunch integrate`](/docs/cli/integrate.md) - Link with platform
- [`forklaunch deploy create`](/docs/cli/deploy.md) - Deploy releases
- [`forklaunch environment sync`](/docs/cli/environment.md) - Manage environment variables

## See Also

- [Deploy Command](/docs/cli/deploy.md)
- [Release and Deploy Guide](/docs/guides/release-and-deploy.md)
- [CI/CD Integration](/docs/guides/cicd.md)
- [Semantic Versioning](https://semver.org/)
