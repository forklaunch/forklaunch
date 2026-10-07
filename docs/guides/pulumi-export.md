---
title: Infrastructure Deployment
category: Guides
description: Infrastructure deployment approaches for ForkLaunch applications.
---

## Overview

ForkLaunch currently does **not** include built-in infrastructure-as-code (IaC) tooling like Pulumi or Terraform. Infrastructure provisioning for production environments is handled separately from the application code.

## Current Infrastructure Approach

### Local Development

For local development, ForkLaunch automatically generates `docker-compose.yaml` files with the necessary infrastructure services:

```yaml
# Auto-generated docker-compose.yaml
services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: my_app
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

  redis:
    image: redis:7
    ports:
      - "6379:6379"

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    ports:
      - "9000:9000"
      - "9001:9001"
    volumes:
      - minio_data:/data

volumes:
  postgres_data:
  minio_data:
```

Start local infrastructure:

```bash
forklaunch dev
```

### Production Infrastructure

For production deployments, infrastructure must be provisioned separately using:

1. **AWS Console** - Manual provisioning through AWS web interface
2. **Terraform** - Write your own Terraform configurations
3. **Pulumi** - Write your own Pulumi programs
4. **AWS CDK** - Write your own CDK stacks
5. **CloudFormation** - Use CloudFormation templates

### Infrastructure Configuration

ForkLaunch applications connect to infrastructure via environment variables:

```bash
# .env.production
DB_HOST=prod-db.cluster-xyz.us-east-1.rds.amazonaws.com
DB_PORT=5432
DB_USER=prod_user
DB_PASSWORD=${SECRET_DB_PASSWORD}
DB_NAME=my_app

REDIS_URL=redis://prod-cache.abc123.cache.amazonaws.com:6379

S3_REGION=us-east-1
S3_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID}
S3_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY}
S3_BUCKET=prod-my-app-data
```

## Infrastructure as Code (DIY)

If you want to use IaC tools, you'll need to create your own configurations. Here are examples:

### Terraform Example

```hcl
# terraform/main.tf
resource "aws_db_instance" "main" {
  identifier        = "my-app-db"
  engine            = "postgres"
  engine_version    = "15.3"
  instance_class    = "db.t3.micro"
  allocated_storage = 20

  db_name  = "my_app"
  username = "postgres"
  password = var.db_password

  vpc_security_group_ids = [aws_security_group.db.id]
  db_subnet_group_name   = aws_db_subnet_group.main.name

  backup_retention_period = 7
  skip_final_snapshot     = false
}

resource "aws_elasticache_cluster" "redis" {
  cluster_id           = "my-app-cache"
  engine               = "redis"
  node_type            = "cache.t3.micro"
  num_cache_nodes      = 1
  parameter_group_name = "default.redis7"
  port                 = 6379
  security_group_ids   = [aws_security_group.cache.id]
  subnet_group_name    = aws_elasticache_subnet_group.main.name
}

resource "aws_s3_bucket" "data" {
  bucket = "my-app-data"

  versioning {
    enabled = true
  }

  server_side_encryption_configuration {
    rule {
      apply_server_side_encryption_by_default {
        sse_algorithm = "AES256"
      }
    }
  }
}
```

### Pulumi Example

```typescript
// pulumi/index.ts
import * as aws from "@pulumi/aws";
import * as pulumi from "@pulumi/pulumi";

// RDS PostgreSQL
const db = new aws.rds.Instance("my-app-db", {
  engine: "postgres",
  engineVersion: "15.3",
  instanceClass: "db.t3.micro",
  allocatedStorage: 20,
  dbName: "my_app",
  username: "postgres",
  password: config.requireSecret("dbPassword"),
  vpcSecurityGroupIds: [dbSecurityGroup.id],
  dbSubnetGroupName: dbSubnetGroup.name,
  backupRetentionPeriod: 7,
  skipFinalSnapshot: false,
});

// ElastiCache Redis
const cache = new aws.elasticache.Cluster("my-app-cache", {
  engine: "redis",
  nodeType: "cache.t3.micro",
  numCacheNodes: 1,
  parameterGroupName: "default.redis7",
  port: 6379,
  securityGroupIds: [cacheSecurityGroup.id],
  subnetGroupName: cacheSubnetGroup.name,
});

// S3 Bucket
const bucket = new aws.s3.Bucket("my-app-data", {
  versioning: {
    enabled: true,
  },
  serverSideEncryptionConfiguration: {
    rule: {
      applyServerSideEncryptionByDefault: {
        sseAlgorithm: "AES256",
      },
    },
  },
});

// Export connection details
export const dbEndpoint = db.endpoint;
export const cacheEndpoint = cache.cacheNodes[0].address;
export const bucketName = bucket.id;
```

## Environment-Specific Configuration

ForkLaunch applications adapt to different environments through environment variables:

### Development

```bash
# .env.development
DB_HOST=localhost
DB_PORT=5432
REDIS_URL=redis://localhost:6379
S3_URL=http://localhost:9000  # MinIO
```

### Staging

```bash
# .env.staging
DB_HOST=staging-db.us-east-1.rds.amazonaws.com
DB_PORT=5432
REDIS_URL=redis://staging-cache.amazonaws.com:6379
S3_BUCKET=staging-my-app-data
```

### Production

```bash
# .env.production
DB_HOST=prod-db.us-east-1.rds.amazonaws.com
DB_PORT=5432
REDIS_URL=redis://prod-cache.amazonaws.com:6379
S3_BUCKET=prod-my-app-data
```

## Best Practices

### 1. Separate Infrastructure from Application Code

Keep IaC configurations in a separate repository or directory:

```
my-app/
├── src/                    # Application code
├── terraform/              # Infrastructure definitions
│   ├── modules/
│   │   ├── database/
│   │   ├── cache/
│   │   └── storage/
│   ├── environments/
│   │   ├── dev.tfvars
│   │   ├── staging.tfvars
│   │   └── prod.tfvars
│   └── main.tf
└── docker-compose.yaml     # Local development only
```

### 2. Use Infrastructure Modules

Create reusable modules for common patterns:

```hcl
# terraform/modules/postgres/main.tf
resource "aws_db_instance" "this" {
  identifier        = var.identifier
  engine            = "postgres"
  engine_version    = var.engine_version
  instance_class    = var.instance_class
  allocated_storage = var.allocated_storage
  # ...
}
```

### 3. Manage Secrets Properly

Never commit secrets to version control:

```bash
# Use AWS Secrets Manager
DB_PASSWORD=$(aws secretsmanager get-secret-value --secret-id prod/db/password --query SecretString --output text)

# Or environment-specific secret stores
export DB_PASSWORD=${PROD_DB_PASSWORD}  # From CI/CD
```

### 4. Document Infrastructure Requirements

Document what infrastructure your application needs:

```yaml
# .forklaunch/manifest.toml
[project.resources]
database = "PostgreSQL"
cache = "redis"
object_store = "s3"

[project.infrastructure]
database_instance_class = "db.t3.micro"
cache_node_type = "cache.t3.micro"
s3_versioning = true
backup_retention_days = 7
```

## Future Plans

ForkLaunch may add infrastructure export capabilities in the future. Planned features include:

- Terraform module generation from manifest
- Pulumi program generation
- AWS CDK stack generation
- CloudFormation template export

Check the roadmap for updates on infrastructure tooling.

## Related Documentation

- [Infrastructure Overview](/docs/infrastructure/overview.md)
- [Databases](/docs/infrastructure/databases.md)
- [Caches](/docs/infrastructure/caches.md)
- [Storage](/docs/infrastructure/storage.md)
- [Dependency Management](/docs/guides/dependency-management.md)
