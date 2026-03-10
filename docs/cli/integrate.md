---
title: Integrate Command
category: CLI Reference
description: Link local application with ForkLaunch platform for deployment and release management.
---

## Overview

The `integrate` command connects your local ForkLaunch application to a platform application, enabling release creation, deployments, and environment management through the platform.

## Usage

```bash
forklaunch integrate --app <application-id> [options]
```

## Arguments

**Required:**
- `-a, --app <application-id>` - Platform application ID to link to

**Optional:**
- `-p, --path <path>` - Path to application root (defaults to current directory)

## What It Does

When you run `integrate`, the CLI:

1. **Validates Platform Connection**: Confirms the application exists on the platform
2. **Updates Local Manifest**: Adds platform IDs to `.forklaunch/manifest.toml`
3. **Enables Platform Features**: Unlocks `release` and `deploy` commands

## Prerequisites

- You must be logged in: `forklaunch login`
- An application must be created on the platform first
- Your account must have access to the application

## Step-by-Step Guide

### 1. Create Platform Application

First, create an application on the platform at https://platform.forklaunch.com:

1. Navigate to Applications → Create New
2. Enter application name and description
3. Copy the Application ID from the URL or application details

### 2. Link Local Application

In your local application directory:

```bash
forklaunch integrate --app app_abc123xyz
```

**Example Output:**
```
[INFO] Validating application on platform...
[OK] Found application: my-backend-api

[OK] Application integrated successfully!

[INFO] Platform App ID: app_abc123xyz
[INFO] Application Name: my-backend-api
[INFO] Organization ID: org_def456uvw

[INFO] You can now use:
  forklaunch release create --version <version>
  forklaunch deploy create --release <version> --environment <env> --region <region>
```

### 3. Verify Integration

Check your manifest file (`.forklaunch/manifest.toml`):

```toml
[application]
name = "my-backend-api"
platform_application_id = "app_abc123xyz"
platform_organization_id = "org_def456uvw"

# ... rest of manifest
```

## What Gets Updated

The `integrate` command modifies only your local `.forklaunch/manifest.toml` file:

**Before:**
```toml
[application]
name = "my-backend-api"
description = "Backend API services"
```

**After:**
```toml
[application]
name = "my-backend-api"
description = "Backend API services"
platform_application_id = "app_abc123xyz"
platform_organization_id = "org_def456uvw"
```

## Common Use Cases

### Fresh Application Setup

For a new application that needs platform integration:

```bash
# Initialize locally
forklaunch init app my-app

# Create services
forklaunch init service users
forklaunch init service payments

# Create platform application via UI, then integrate
forklaunch login
forklaunch integrate --app app_abc123xyz
```

### Migrating Existing Application

For an existing application moving to the platform:

```bash
# In existing application directory
forklaunch login
forklaunch integrate --app app_abc123xyz

# Create first release
forklaunch release create --version 1.0.0

# Deploy to staging
forklaunch deploy create --release 1.0.0 --environment staging --region us-west-2
```

### Team Onboarding

When a new developer joins:

```bash
# Clone repository
git clone <repo>
cd <repo>

# Integration already in manifest.toml
# Just login and start working
forklaunch login
forklaunch env sync --environment development --region us-west-2
pnpm dev
```

## Error Handling

### Application Not Found

```bash
[ERROR] Failed to find application: app_invalid123 (Status: 404)
```

**Solution**: Verify the application ID is correct and you have access.

### Not Logged In

```bash
[ERROR] Not authenticated. Run: forklaunch login
```

**Solution**: Login first with `forklaunch login`

### Permission Denied

```bash
[ERROR] Failed to access application (Status: 403)
```

**Solution**: Ask organization admin to grant you access to the application.

### Already Integrated

If your application is already integrated, running `integrate` again will update the IDs (useful if you need to change the linked platform application).

## Next Steps After Integration

Once integrated, you can:

1. **Create Releases**:
   ```bash
   forklaunch release create --version 1.0.0 --notes "Initial release"
   ```

2. **Deploy to Environments**:
   ```bash
   forklaunch deploy create --release 1.0.0 --environment staging --region us-west-2
   ```

3. **Manage Environment Variables**:
   ```bash
   forklaunch env sync --environment development --region us-west-2
   ```

4. **View Application on Platform**:
   Visit `https://platform.forklaunch.com/applications/app_abc123xyz`

## Related Commands

- [`forklaunch login`](/docs/cli/authentication.md#login) - Authenticate with platform
- [`forklaunch release create`](/docs/cli/release.md#create) - Create releases
- [`forklaunch deploy create`](/docs/cli/deploy.md#create) - Deploy releases
- [`forklaunch environment sync`](/docs/cli/environment.md#sync) - Sync environment variables

## Best Practices

1. **Integrate Early**: Link with platform before creating releases
2. **Team Agreement**: Ensure whole team uses same platform application
3. **Commit Manifest**: Commit the updated `.forklaunch/manifest.toml` to version control
4. **Document App ID**: Keep application ID in team documentation for reference
5. **One Platform App Per Environment**: Consider separate platform apps for staging vs production if needed

## See Also

- [Getting Started Guide](/docs/getting-started.md)
- [Release and Deploy Guide](/docs/guides/release-and-deploy.md)
- [Platform Integration Overview](/docs/guides/platform-integration.md)
