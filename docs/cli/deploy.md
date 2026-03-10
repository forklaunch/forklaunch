---
title: Deploy Command
category: CLI Reference
description: Deploy releases to environments and manage running deployments.
---

## Overview

The `deploy` command takes a release and deploys it to a specific environment and region, configuring environment variables, and starting your services and workers.

## Usage

```bash
forklaunch deploy <subcommand> [options]
```

## Subcommands

### create

Deploy a release to an environment.

```bash
forklaunch deploy create --release <version> --environment <env> --region <region> [options]
```

**Required:**
- `-r, --release <version>` - Release version to deploy (e.g., `1.0.0`)
- `-e, --environment <env>` - Target environment (e.g., `staging`, `production`)
- `--region <region>` - Deployment region (e.g., `us-east-1`, `eu-west-1`)

**Optional:**
- `-p, --path <path>` - Path to application root
- `--distribution-config <config>` - Distribution strategy (`centralized` or `distributed`)
- `--dry-run` - Preview deployment without executing it
- `--full` - Force a full deployment with Pulumi state refresh
- `--node-env <env>` - NODE_ENV for this deployment (`production` or `development`)
- `--no-wait` - Don't wait for deployment to complete

### destroy

Tear down application infrastructure for a given environment and region.

```bash
forklaunch deploy destroy --environment <env> --region <region> [options]
```

**Required:**
- `-e, --environment <env>` - Environment name (e.g., `staging`, `production`)
- `--region <region>` - AWS region (e.g., `us-east-1`)

**Optional:**
- `-p, --path <path>` - Path to application root
- `--mode <mode>` - Destroy mode: `all` (default) or `preserve-data`
- `--no-wait` - Don't wait for destruction to complete

## Creating Deployments

### Basic Deployment

```bash
$ forklaunch deploy create \
  --release 1.0.0 \
  --environment staging \
  --region us-west-2

[INFO] Creating deployment for release 1.0.0...
[INFO] Environment: staging
[INFO] Region: us-west-2

[INFO] Validating release...
[OK] Release 1.0.0 found

[INFO] Checking environment variables...
[WARNING] Missing environment variables detected:
  Service 'payments' requires:
    - STRIPE_SECRET_KEY
  Worker 'email-processor' requires:
    - SENDGRID_API_KEY

[PROMPT] Enter STRIPE_SECRET_KEY (for service: payments): **********************
[PROMPT] Enter SENDGRID_API_KEY (for worker: email-processor): **********************

[OK] All environment variables configured

[INFO] Deploying services...
[OK] Service 'users' deployed (2 instances)
[OK] Service 'payments' deployed (2 instances)
[OK] Service 'notifications' deployed (1 instance)

[INFO] Deploying workers...
[OK] Worker 'email-processor' deployed (3 instances)
[OK] Worker 'data-export' deployed (1 instance)

[INFO] Configuring networking...
[OK] Load balancer configured
[OK] DNS records updated
[OK] SSL certificates provisioned

[OK] Deployment created successfully!

[INFO] Deployment ID: dep_abc123xyz
[INFO] Status: running
[INFO] URL: https://staging-api.example.com
[INFO] Dashboard: https://platform.forklaunch.com/deployments/dep_abc123xyz

[INFO] Services:
  - users: https://staging-api.example.com/users
  - payments: https://staging-api.example.com/payments
  - notifications: https://staging-api.example.com/notifications
```

### Dry Run

Preview what a deployment would do without executing it:

```bash
$ forklaunch deploy create \
  --release 1.0.0 \
  --environment staging \
  --region us-west-2 \
  --dry-run
```

### Multi-Region Deployment

Deploy the same release to multiple regions:

```bash
# US West
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region us-west-2

# EU West
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region eu-west-1

# Asia Pacific
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region ap-southeast-1
```

## Environment Variable Management

### Interactive Prompts

When deploying, ForkLaunch detects missing environment variables and prompts you:

```bash
[WARNING] Missing environment variables detected:

Service 'payments' requires:
  - STRIPE_SECRET_KEY (payment processing)
  - STRIPE_WEBHOOK_SECRET (webhook verification)

Worker 'email-processor' requires:
  - SENDGRID_API_KEY (email sending)

[PROMPT] Enter STRIPE_SECRET_KEY: **********************
[PROMPT] Enter STRIPE_WEBHOOK_SECRET: **********************
[PROMPT] Enter SENDGRID_API_KEY: **********************

[OK] Environment variables saved securely
```

### Pre-configured Variables

Variables already set on the platform skip prompts:

```bash
[INFO] Checking environment variables...
[OK] All required variables configured
  - DATABASE_URL ✓
  - REDIS_URL ✓
  - STRIPE_SECRET_KEY ✓
  - SENDGRID_API_KEY ✓
```

## Deployment Lifecycle

### Phases

1. **Validation** - Verify release exists and is valid
2. **Environment Check** - Ensure all environment variables are configured
3. **Service Deployment** - Deploy and start services
4. **Worker Deployment** - Deploy and start workers
5. **Networking Configuration** - Setup load balancers, DNS, SSL
6. **Health Checks** - Verify all components are healthy
7. **Traffic Routing** - Route traffic to new deployment

### Monitoring Progress

View deployment progress on the platform:

```bash
[INFO] Dashboard: https://platform.forklaunch.com/deployments/dep_abc123xyz
```

## Destroying Deployments

### Basic Destroy

```bash
$ forklaunch deploy destroy --environment staging --region us-west-2

[WARNING] This will destroy infrastructure for staging (us-west-2)
[INFO] Destroying deployment...
[OK] Stopped all services
[OK] Stopped all workers
[OK] Deleted load balancer
[OK] Removed DNS records

[OK] Deployment destroyed successfully
```

### Preserve Data

Keep databases and storage while tearing down compute:

```bash
forklaunch deploy destroy --environment staging --region us-west-2 --mode preserve-data
```

## Environment Configuration

When you deploy with ForkLaunch, you'll need to configure environment variables for your external infrastructure and services. Ensure you have:

- Database connection strings (if using databases)
- Redis URLs (if using caching)
- S3 credentials and bucket names (if using object storage)
- API keys for external services
- Any other service-specific configuration

## Rollback

### Manual Rollback

Roll back to previous release:

```bash
# Redeploy previous release
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region us-west-2
```

## Common Workflows

### Initial Deployment

```bash
# 1. Create and integrate application
forklaunch init app my-app
forklaunch integrate --app app_abc123

# 2. Develop and test locally
# ... build features ...

# 3. Create release
forklaunch release create --version 1.0.0

# 4. Deploy to development
forklaunch deploy create \
  --release 1.0.0 \
  --environment development \
  --region us-west-2

# 5. Deploy to staging
forklaunch deploy create \
  --release 1.0.0 \
  --environment staging \
  --region us-west-2

# 6. Test in staging
# ... run tests ...

# 7. Deploy to production
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region us-west-2
```

### Update Workflow

```bash
# 1. Create new release
forklaunch release create --version 1.1.0

# 2. Deploy to staging first
forklaunch deploy create \
  --release 1.1.0 \
  --environment staging \
  --region us-west-2

# 3. Test in staging
# ... verify changes ...

# 4. Deploy to production
forklaunch deploy create \
  --release 1.1.0 \
  --environment production \
  --region us-west-2
```

### Hotfix Workflow

```bash
# 1. Create hotfix release
forklaunch release create --version 1.0.1 --notes "Critical bug fix"

# 2. Deploy directly
forklaunch deploy create \
  --release 1.0.1 \
  --environment production \
  --region us-west-2
```

## Troubleshooting

### Missing Environment Variables

```bash
[ERROR] Deployment blocked: Missing environment variables
```

**Solution**: Set variables on platform or provide during deployment.

### Health Check Failures

```bash
[ERROR] Health check failed for service: users
```

**Solution**: Check logs, fix issues, redeploy.

### Insufficient Capacity

```bash
[ERROR] Insufficient capacity in region us-west-2
```

**Solution**: Deploy to different region or contact support.

### Database Connection Errors

```bash
[ERROR] Cannot connect to database
```

**Solution**: Verify environment variables, check security groups.

## Best Practices

1. **Test in Staging**: Always deploy to staging before production
2. **Monitor Deployments**: Watch health metrics during deployment
3. **Environment Parity**: Keep staging similar to production
4. **Version Control**: Tag releases in Git matching deployment versions
5. **Document Changes**: Maintain release notes for each deployment
6. **Schedule Deploys**: Deploy during low-traffic windows
7. **Backup Data**: Always backup before destructive operations
8. **Communication**: Notify team before production deployments

## Related Commands

- [`forklaunch release create`](/docs/cli/release.md) - Create releases
- [`forklaunch environment sync`](/docs/cli/environment.md) - Manage environment variables
- [`forklaunch integrate`](/docs/cli/integrate.md) - Link with platform

## See Also

- [Release Command](/docs/cli/release.md)
- [Release and Deploy Guide](/docs/guides/release-and-deploy.md)
- [Environment Management](/docs/guides/environment-management.md)
