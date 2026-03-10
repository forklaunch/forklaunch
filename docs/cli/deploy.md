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
- `-e, --environment <env>` - Target environment (`development`, `staging`, `production`)
- `--region <region>` - Deployment region (e.g., `us-west-2`, `eu-west-1`)

**Optional:**
- `-p, --path <path>` - Path to application root
- `--distribution-config <config>` - Custom distribution configuration (advanced)
- `--priority <level>` - Deployment priority: `low`, `normal`, `high` (default: `normal`)

### destroy

Tear down a deployment and release all resources.

```bash
forklaunch deploy destroy --deployment <deployment-id> [options]
```

**Required:**
- `-d, --deployment <deployment-id>` - Deployment ID to destroy

**Optional:**
- `-p, --path <path>` - Path to application root
- `--force` - Skip confirmation prompt

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

### High Priority Deployment

For urgent hotfixes or critical updates:

```bash
$ forklaunch deploy create \
  --release 1.0.1 \
  --environment production \
  --region us-west-2 \
  --priority high

[INFO] Creating HIGH PRIORITY deployment...
[INFO] This deployment will be prioritized over others
[...]
[OK] Deployment created successfully!
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

### Validation

ForkLaunch validates environment variable values:

```bash
[PROMPT] Enter DATABASE_PORT: abc
[ERROR] Invalid value: DATABASE_PORT must be a number

[PROMPT] Enter DATABASE_PORT: 5432
[OK] Valid value
```

### Updating Variables

Update variables for existing deployment:

```bash
# Update via platform UI or CLI
forklaunch environment sync \
  --environment staging \
  --region us-west-2

# Redeploy to apply changes
forklaunch deploy create \
  --release 1.0.0 \
  --environment staging \
  --region us-west-2
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

Or use the CLI:

```bash
# Check deployment status
forklaunch deployment status dep_abc123xyz

# View logs
forklaunch deployment logs dep_abc123xyz --service users

# View metrics
forklaunch deployment metrics dep_abc123xyz
```

## Destroying Deployments

### Basic Destroy

```bash
$ forklaunch deploy destroy --deployment dep_abc123xyz

[WARNING] This will destroy deployment dep_abc123xyz
[WARNING] All data will be lost
[PROMPT] Are you sure? (yes/no): yes

[INFO] Destroying deployment...
[OK] Stopped all services
[OK] Stopped all workers
[OK] Deleted load balancer
[OK] Removed DNS records
[OK] Deleted databases (data backed up to S3)
[OK] Deleted caches
[OK] Deleted storage buckets (data backed up)

[OK] Deployment destroyed successfully

[INFO] Backups available for 30 days at:
  - Database: s3://forklaunch-backups/dep_abc123xyz/database
  - Files: s3://forklaunch-backups/dep_abc123xyz/storage
```

### Force Destroy

Skip confirmation:

```bash
forklaunch deploy destroy --deployment dep_abc123xyz --force
```

### Destroy with Data Retention

Preserve data for disaster recovery:

```bash
$ forklaunch deploy destroy --deployment dep_abc123xyz --retain-data

[INFO] Destroying deployment (retaining data)...
[OK] Stopped services and workers
[OK] Removed networking
[OK] Kept databases (marked for retention)
[OK] Kept storage buckets

[INFO] Data retained at:
  - postgres://...
  - s3://...

[INFO] Data will be retained for 90 days
[INFO] To restore, create new deployment with --restore-from dep_abc123xyz
```

## Environment Configuration

When you deploy with ForkLaunch, you'll need to configure environment variables for your external infrastructure and services. Ensure you have:

- Database connection strings (if using databases)
- Redis URLs (if using caching)
- S3 credentials and bucket names (if using object storage)
- API keys for external services
- Any other service-specific configuration

## Deployment Strategies

### Rolling Deployment (Default)

Updates instances gradually:

```bash
# Deploy with rolling updates
forklaunch deploy create \
  --release 1.1.0 \
  --environment production \
  --region us-west-2

[INFO] Using rolling deployment strategy
[INFO] Updating 1 instance at a time
[OK] Instance 1/3 updated
[OK] Instance 2/3 updated
[OK] Instance 3/3 updated
```

### Blue-Green Deployment

Zero-downtime with instant rollback:

```bash
# Deploy to green environment
forklaunch deploy create \
  --release 1.1.0 \
  --environment production \
  --region us-west-2 \
  --strategy blue-green

[INFO] Creating green environment...
[OK] Green environment ready
[INFO] Routing 10% traffic to green...
[INFO] Routing 50% traffic to green...
[INFO] Routing 100% traffic to green...
[OK] Blue environment decommissioned
```

### Canary Deployment

Gradual traffic shift:

```bash
forklaunch deploy create \
  --release 1.1.0 \
  --environment production \
  --region us-west-2 \
  --strategy canary \
  --canary-percentage 10

[INFO] Deploying canary (10% traffic)...
[OK] Canary deployed
[INFO] Monitor metrics at: https://platform.forklaunch.com/deployments/...
[INFO] Promote with: forklaunch deployment promote dep_abc123xyz
```

## Rollback

### Automatic Rollback

If health checks fail, automatic rollback:

```bash
[INFO] Deploying release 1.1.0...
[ERROR] Health check failed for service: payments
[WARNING] Initiating automatic rollback...
[OK] Rolled back to release 1.0.0
[INFO] Deployment failed - check logs for details
```

### Manual Rollback

Roll back to previous release:

```bash
# Redeploy previous release
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region us-west-2
```

## Monitoring Deployments

### View Status

```bash
$ forklaunch deployment status dep_abc123xyz

Deployment: dep_abc123xyz
Status: running
Release: 1.0.0
Environment: production
Region: us-west-2
Created: 2026-01-20 19:00:00 UTC

Services:
  users (2/2 healthy)
  payments (2/2 healthy)
  notifications (1/1 healthy)

Workers:
  email-processor (3/3 healthy)
  data-export (1/1 healthy)

Health: ✓ All systems operational
```

### View Logs

```bash
# Service logs
forklaunch deployment logs dep_abc123xyz --service payments

# Worker logs
forklaunch deployment logs dep_abc123xyz --worker email-processor

# All logs
forklaunch deployment logs dep_abc123xyz --all

# Follow logs
forklaunch deployment logs dep_abc123xyz --follow
```

### View Metrics

```bash
# View metrics
forklaunch deployment metrics dep_abc123xyz

# CPU usage
forklaunch deployment metrics dep_abc123xyz --metric cpu

# Memory usage
forklaunch deployment metrics dep_abc123xyz --metric memory

# Request rate
forklaunch deployment metrics dep_abc123xyz --metric requests
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

# 5. Monitor
forklaunch deployment status <deployment-id>
```

### Hotfix Workflow

```bash
# 1. Create hotfix release
forklaunch release create --version 1.0.1 --notes "Critical bug fix"

# 2. Deploy with high priority
forklaunch deploy create \
  --release 1.0.1 \
  --environment production \
  --region us-west-2 \
  --priority high

# 3. Monitor closely
forklaunch deployment logs <deployment-id> --follow
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
3. **Gradual Rollout**: Use canary or blue-green for production
4. **Environment Parity**: Keep staging similar to production
5. **Automated Rollback**: Configure automatic rollback on failures
6. **Version Control**: Tag releases in Git matching deployment versions
7. **Document Changes**: Maintain release notes for each deployment
8. **Schedule Deploys**: Deploy during low-traffic windows
9. **Backup Data**: Always backup before destructive operations
10. **Communication**: Notify team before production deployments

## Related Commands

- [`forklaunch release create`](/docs/cli/release.md) - Create releases
- [`forklaunch environment sync`](/docs/cli/environment.md) - Manage environment variables
- [`forklaunch integrate`](/docs/cli/integrate.md) - Link with platform

## See Also

- [Release Command](/docs/cli/release.md)
- [Release and Deploy Guide](/docs/guides/release-and-deploy.md)
- [Environment Management](/docs/guides/environment-management.md)
- [Monitoring and Observability](/docs/guides/monitoring.md)
