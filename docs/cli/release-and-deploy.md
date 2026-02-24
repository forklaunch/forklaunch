---
title: Release and Deploy
category: CLI
description: Deploy your ForkLaunch applications to production with free tier defaults
---

## Overview

ForkLaunch provides simple CLI commands to release and deploy your applications to AWS with **zero infrastructure configuration** required. The platform automatically provisions resources using free tier defaults.

---

## Prerequisites

1. **Authenticated**: Run `forklaunch login` first
2. **Platform Application**: Create an application in the Platform UI
3. **Git Repository**: Your application should be in a git repository

---

## Environment Variables

The CLI supports configuration via environment variables.

| Variable | Purpose | Default | Example |
|----------|---------|---------|---------|
| `FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL` | Platform management API base URL | Installed: `https://platform.forklaunch.com`<br/>Dev build: `http://localhost:8004` | `https://platform.forklaunch.com` |
| `FORKLAUNCH_IAM_API_URL` | IAM API base URL (used for token refresh) | Installed: `https://iam.forklaunch.com`<br/>Dev build: `http://localhost:8000` | `https://iam.forklaunch.com` |
| `FORKLAUNCH_PLATFORM_UI_URL` | Platform UI base URL (links in CLI output) | Installed: `https://forklaunch.com`<br/>Dev build: `http://localhost:3001` | `https://forklaunch.com` |
| `FORKLAUNCH_HMAC_SECRET` | Use HMAC auth mode (CI/CD) instead of user login | None | `super-secret` |

**Auth token**

- For interactive use, `forklaunch login` stores credentials in `~/.forklaunch/token` (you typically do not set a token env var manually).

**Usage:**
```bash
# Point to local development platform
export FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL=http://localhost:8004
export FORKLAUNCH_IAM_API_URL=http://localhost:8000
export FORKLAUNCH_PLATFORM_UI_URL=http://localhost:3001

# Point to staging platform
export FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL=https://staging-platform.forklaunch.io
export FORKLAUNCH_IAM_API_URL=https://staging-iam.forklaunch.io
export FORKLAUNCH_PLATFORM_UI_URL=https://staging.forklaunch.io

# Point to self-hosted enterprise platform
export FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL=https://forklaunch.company.com
export FORKLAUNCH_IAM_API_URL=https://iam.forklaunch.company.com
export FORKLAUNCH_PLATFORM_UI_URL=https://forklaunch.company.com
```

---

## Quick Start

### 1. Integrate with Platform

Link your local application to the platform:

```bash
forklaunch integrate --app e1d113dc-cb1e-4b33-bb92-4657d3e0ce3d
```

This stores the application ID in your `.forklaunch/manifest.toml` for subsequent commands.

### 2. Check Required Environment Variables

Before creating a release, check what environment variables your application needs:

```bash
forklaunch environment validate
```

This command scans your `registrations.ts` files and shows:
- All required environment variables
- Which variables are missing
- Which projects need each variable

**Example output:**
```
[INFO] Validating environment variables...
Workspace: /path/to/my-app
Modules path: /path/to/my-app/src/modules

3 projects found:
  - iam-base
  - core
  - monitoring

[OK] All environment variables are defined!
```

Or if variables are missing:
```
[WARN] Missing environment variables:

iam-base:
  - JWT_SECRET (used in: registrations.ts:45)
  - DATABASE_URL (used in: registrations.ts:32)

[INFO] Run 'forklaunch environment sync' to add missing variables with blank values
```

### 3. Create a Release

Package your code and upload to the platform:

```bash
forklaunch release create --version 1.0.0
```

This will:
- Sync projects with your manifest (non-interactive)
- Auto-detect git commit and branch
- Export OpenAPI specifications
- **Auto-detect required environment variables** (scans all `registrations.ts` files)
- Generate release manifest with env var requirements
- Detect runtime dependencies, integrations, and service mesh connections
- Upload to platform

**Example output:**
```
[INFO] Syncing projects with manifest...

[OK] Sync completed - no changes detected

[INFO] Creating release 1.0.0...

  Detecting git metadata... [OK]
[INFO] Commit: abc12345 (main)
[INFO] Exporting OpenAPI specifications... [OK] (2 services)
[INFO] Detecting required environment variables... [OK] (5 variables)
[INFO] Detecting runtime dependencies... [OK] (3 resources)
[INFO] Detecting integrations... [OK] (1 integrations)
[INFO] Detecting worker configurations... [OK] (1 workers)
[INFO] Detecting service mesh connections... [OK] (2 connections)
[INFO] Generating release manifest... [OK]
[INFO] Uploading release to platform... [OK]

[OK] Release 1.0.0 created successfully!
```

The platform now knows your application requires these environment variables:
- `DATABASE_URL`
- `JWT_SECRET`
- `OTEL_SERVICE_NAME`
- `OTEL_EXPORTER_OTLP_ENDPOINT`
- Any other variables from your `registrations.ts` files

### 4. Set Environment Variables

Go to Platform UI and set the required environment variables found in step 2:
- `DATABASE_URL`
- `JWT_SECRET`
- Any service-specific variables

### 5. Deploy

Deploy your release to an environment:

```bash
forklaunch deploy create --release 1.0.0 --environment production --region us-east-1
```

The platform will provision **free tier resources** by default:
- Database: db.t3.micro (750 hours/month free)
- Services: 256m CPU, 512Mi RAM (Fargate free tier)
- Auto-scaling: 1-2 replicas
- Load balancer with auto-SSL
- Monitoring: OTEL, Prometheus, Grafana

---

## Commands

### forklaunch integrate

Link local application to platform application.

**Usage**:
```bash
forklaunch integrate --app <application-id> [--path <path>]
```

**Options**:
| Option | Short | Description |
|--------|-------|-------------|
| `--app` | `-a` | Platform application ID (required) |
| `--path` | `-p` | Application root path (optional) |

**Example**:
```bash
forklaunch integrate --app e1d113dc-cb1e-4b33-bb92-4657d3e0ce3d
```

**Output**:
```
Validating application on platform...
✓ Found application: my-app
✓ Application integrated successfully!
  Platform App ID: e1d113dc-cb1e-4b33-bb92-4657d3e0ce3d
  Application Name: my-app
  Organization ID: org-acme-corp

You can now use:
  forklaunch release create --version <version>
  forklaunch deploy create --release <version> --environment <env> --region <region>
```

---

### forklaunch openapi export

Export OpenAPI 3.1 specifications from your services.

**Usage**:
```bash
forklaunch openapi export [--output <directory>]
```

**Options**:
| Option | Short | Description |
|--------|-------|-------------|
| `--output` | `-o` | Output directory (default: `.forklaunch/openapi`) |
| `--path` | `-p` | Application root path (optional) |

**Example**:
```bash
forklaunch openapi export
```

**Output**:
```
Exporting OpenAPI specifications...

  - iam-base
  - billing-base

[OK] Successfully exported 2 OpenAPI specification(s)
  Output: /path/to/app/.forklaunch/openapi
```

**Files Created**:
```
.forklaunch/openapi/
├── iam-base/openapi.json
└── billing-base/openapi.json
```

---

### forklaunch release create

Create a new release and upload to the platform.

**Usage**:
```bash
forklaunch release create --version <version> [options]
```

**Options**:
| Option | Short | Description |
|--------|-------|-------------|
| `--version` | `-v` | Release version (required) |
| `--notes` | `-n` | Release notes (optional) |
| `--path` | `-p` | Application root path (optional) |
| `--dry-run` | - | Simulate without uploading |
| `--local` | - | Package local code and upload to S3 (for CI/CD testing without GitHub) |
| `--skip-sync` | - | Skip automatic sync of projects with manifest before creating release |

**Example**:
```bash
forklaunch release create --version 1.0.0 --notes "Initial release"
```

**Output**:
```
Creating release 1.0.0...

  Detecting git metadata... ✓
    Commit: abc123de (main)
  Exporting OpenAPI specifications... ✓
  Generating release manifest... ✓
  Uploading release to platform... ✓

✓ Release 1.0.0 created successfully!

Next steps:
  1. Set environment variables in Platform UI
  2. forklaunch deploy create --release 1.0.0 --environment <env> --region <region>
```

**Dry Run Mode**:
```bash
forklaunch release create --version 1.0.0 --dry-run
```

Generates manifest locally at `.forklaunch/release-manifest.json` without uploading.

---

### forklaunch deploy create

Deploy a release to an environment and region.

**Usage**:
```bash
forklaunch deploy create --release <version> --environment <env> --region <region> [options]
```

**Options**:
| Option | Short | Description |
|--------|-------|-------------|
| `--release` | `-r` | Release version to deploy (required) |
| `--environment` | `-e` | Environment name (required) |
| `--region` | - | AWS region (required) |
| `--distribution-config` | - | Distribution strategy (default: `centralized`) |
| `--path` | `-p` | Application root path (optional) |
| `--no-wait` | - | Don't wait for deployment to complete |

**Example**:
```bash
forklaunch deploy create --release 1.0.0 --environment production --region us-east-1
```

**Output**:
```
Creating deployment: 1.0.0 → production (us-east-1)

[INFO] Triggering deployment... [OK]
[INFO] Deployment ID: dep-abc123

  Validating configuration...
  Provisioning database (RDS PostgreSQL db.t3.micro)...
  Creating load balancer...
  Deploying services (256m CPU, 512Mi RAM)...
  Configuring auto-scaling (1-2 replicas)...
  Setting up monitoring (OTEL, Prometheus, Grafana)...

[OK] Operation successful!

[INFO] API: https://my-app.production.forklaunch.io
[INFO] Docs: https://my-app.production.forklaunch.io/docs
```

**No-Wait Mode**:
```bash
forklaunch deploy create --release 1.0.0 --environment staging --region us-east-1 --no-wait
```

Triggers deployment and returns immediately (check status in Platform UI).

---

### forklaunch deploy destroy

Destroy infrastructure for an environment and region.

**Usage**:
```bash
forklaunch deploy destroy --environment <env> --region <region> [options]
```

**Options**:
| Option | Short | Description |
|--------|-------|-------------|
| `--environment` | `-e` | Environment name (required) |
| `--region` | - | AWS region (required) |
| `--mode` | - | Destroy mode: `all` (default) or `preserve-data` |
| `--path` | `-p` | Application root path (optional) |
| `--no-wait` | - | Don't wait for destruction to complete |

**Example**:
```bash
forklaunch deploy destroy --environment staging --region us-east-1 --mode preserve-data
```

## Common Workflows

### First Deployment

```bash
# 1. Init app
forklaunch init application my-app --path ./my-app --database postgresql --modules iam-base
cd my-app

# 2. Integrate
forklaunch integrate --app <app-id-from-platform-ui>

# 3. Create release
forklaunch release create --version 1.0.0

# 4. Set env vars in Platform UI

# 5. Deploy
forklaunch deploy create --release 1.0.0 --environment production --region us-east-1
```

### Multi-Region Deployment

```bash
# Deploy same release to multiple regions
forklaunch deploy create --release 1.0.0 --environment production --region us-east-1
forklaunch deploy create --release 1.0.0 --environment production --region eu-west-1
forklaunch deploy create --release 1.0.0 --environment production --region ap-southeast-1
```

### Staging to Production

```bash
# 1. Deploy to staging
forklaunch release create --version 1.1.0
forklaunch deploy create --release 1.1.0 --environment staging --region us-east-1

# 2. Test in staging...

# 3. Deploy same release to production
forklaunch deploy create --release 1.1.0 --environment production --region us-east-1
```

---

## Default Resources (Free Tier)

When you deploy, the platform automatically provisions:

### Compute (Per Service/Worker)
- **CPU**: 256m (0.25 vCPU)
- **Memory**: 512Mi (0.5 GB)
- **Replicas**: 1-2 (auto-scaling)
- **Cost**: **$0/month** (Fargate free tier)

### Database (PostgreSQL)
- **Instance**: db.t3.micro
- **Storage**: 20 GB
- **Backups**: 7 day retention
- **Availability**: Single AZ
- **Cost**: **$0/month** (750 hours/month free)

### Cache (Redis)
- **Instance**: cache.t3.micro
- **Nodes**: 1
- **Cost**: **$0/month** (free tier eligible)

### Infrastructure
- VPC with public/private subnets
- Application Load Balancer with auto-SSL
- Security groups
- IAM roles
- CloudWatch monitoring

**Total Cost**: **$0/month** for development, **$5-15/month** after free tier

Upgrade resources anytime via Platform UI.

---

## Troubleshooting

### "Application not integrated"
```
Error: Application not integrated with platform.
Run: forklaunch integrate --app <app-id>
```

**Solution**: Run `forklaunch integrate --app <id>` first.

### "No token found"
```
Error: No token found. Please run `forklaunch login` to get a token
```

**Solution**: Run `forklaunch login` to authenticate.

### "Not a git repository"
```
Error: Current directory is not a git repository. Initialize git first.
```

**Solution**: Initialize git with `git init` and make at least one commit.

### OpenAPI Export Fails
```
Error: Service failed to export: ...
```

**Solutions**:
- Ensure service has `package.json` with `dev` script
- Check that all dependencies are installed (`npm install`)
- Verify service code compiles

---

## Best Practices

1. **Version Naming**: Use semantic versioning (1.0.0, 1.0.1, 1.1.0)
2. **Git Commits**: Create releases from tagged commits
3. **Environment Variables**: Set all required vars before deploying
4. **Staging First**: Test in staging before production
5. **Multi-Region**: Deploy gradually (one region, test, then others)
6. **Free Tier**: Start with free tier, monitor costs via Platform UI
7. **Rollback**: Keep previous releases for easy rollback

---

## Examples

See the complete workflow in action:

```bash
# Development workflow
git add .
git commit -m "Add user authentication"
forklaunch release create --version 1.1.0 --notes "Added auth features"
forklaunch deploy create --release 1.1.0 --environment staging --region us-east-1

# Test in staging at: https://my-app.staging.forklaunch.io

# Promote to production
forklaunch deploy create --release 1.1.0 --environment production --region us-east-1
```

---

## Related Documentation

- [Getting Started](../getting-started.md) - Initialize your first application
- [Adding Services](../adding-projects/services.md) - Create new services
- [Adding Workers](../adding-projects/workers.md) - Create background workers
- [Local Development](../local-development.md) - Develop and test locally

---

## Cost Optimization

**Free Tier Limits** (per month):
- ECS Fargate: 20 GB-hours for tasks
- RDS: 750 hours of db.t3.micro
- ElastiCache: Limited free tier
- ALB: 750 hours, 15 GB data
- CloudWatch: 10 custom metrics, 5 GB logs

**Stay within free tier**:
- Use 1-2 service replicas max
- Single database instance
- Minimal worker replicas
- Monitor usage via Platform UI

**When to upgrade**:
- Traffic > 100 requests/second
- Database > 20 GB
- Need multi-AZ for HA
- Upgrade via Platform UI (instant)

---

For questions or issues, see the [Platform Documentation](https://platform.forklaunch.io/docs) or [GitHub Issues](https://github.com/forklaunch/forklaunch-js/issues).

