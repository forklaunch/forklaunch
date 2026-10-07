---
title: "Infrastructure Overview"
description: "Overview of ForkLaunch's infrastructure components: databases, caches, queues, and object storage."
category: "Infrastructure"
---

## Overview

ForkLaunch includes databases, caching, message queues, and object storage. The CLI provisions each component and wires it into your app via dependency injection, so no manual configuration is needed.

When you initialize a new project or service, the CLI sets up the infrastructure you need based on your selections. The platform generates Pulumi IaC code for production deployments on AWS, so your local development environment mirrors what runs in the cloud.

## Components

| Component | Local | Production (AWS) | Package |
|-----------|-------|-------------------|---------|
| **Database** | Docker container | Amazon RDS | `@forklaunch/infrastructure-*` |
| **Cache** | Redis in Docker | Amazon ElastiCache | `@forklaunch/infrastructure-redis` |
| **Queue** | BullMQ (Redis) or Kafka in Docker | Amazon SQS / MSK | `@forklaunch/infrastructure-bullmq` |
| **Object Storage** | MinIO in Docker | Amazon S3 | `@forklaunch/infrastructure-s3` |

## How Infrastructure Is Provisioned

### 1. CLI Selection

When you create an application, the CLI prompts for infrastructure choices:

```bash
forklaunch init application my-app --database postgresql --runtime node
```

Available database options: PostgreSQL, MySQL, MariaDB, MongoDB, Microsoft SQL Server, SQLite, better-sqlite3, libSQL.

### 2. Local Development

Infrastructure runs in Docker containers managed by your development environment. Environment variables are configured automatically:

```bash
# Database
DATABASE_URL=postgresql://user:password@localhost:5432/myapp

# Redis (cache + queues)
REDIS_URL=redis://localhost:6379

# S3 (object storage)
S3_ENDPOINT=http://localhost:9000
S3_ACCESS_KEY_ID=minioadmin
S3_SECRET_ACCESS_KEY=minioadmin
```

### 3. Production Deployment

ForkLaunch generates TypeScript Pulumi code for your full AWS infrastructure:

- **VPC** with public and private subnets
- **ECS Fargate** for containerized services
- **RDS** for relational databases
- **ElastiCache** for Redis
- **MSK** for Kafka (if using Kafka queues)
- **ECR** for container images
- **ALB** for load balancing
- **CloudWatch** for logging and metrics
- **IAM** roles and policies

## Dependency Injection

All infrastructure components are registered in your module's `registrations.ts` and injected into services:

```typescript
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { S3ObjectStore } from '@forklaunch/infrastructure-s3';

// In registrations
const injector = createConfigInjector({
  cache: {
    lifetime: 'singleton',
    factory: () => new RedisTtlCache(60000, otel, { url: REDIS_URL })
  },
  objectStore: {
    lifetime: 'singleton',
    factory: () => new S3ObjectStore(otel, s3Config)
  }
});
```

Services receive infrastructure through their dependency tokens; they never construct clients directly.

## Testing

All infrastructure components can be tested with real Docker containers using `@forklaunch/testing`:

```typescript
import { BlueprintTestHarness } from '@forklaunch/testing';

const harness = new BlueprintTestHarness({
  databaseType: 'postgres',
  needsRedis: true,
  needsS3: true
});

const setup = await harness.setup();
// setup.orm, setup.redis, setup.s3Container all available
```

See the [Testing guide](/docs/guides/testing.md) for full details.

## Related Documentation

- [Databases](/docs/infrastructure/databases.md): Supported databases and ORM patterns
- [Caches](/docs/infrastructure/caches.md): Redis TTL cache system
- [Queues](/docs/infrastructure/queues.md): BullMQ and Kafka message queues
- [Storage](/docs/infrastructure/storage.md): S3 object storage
- [Cache Guide](/docs/guides/cache.md): Detailed cache API reference
- [Testing Guide](/docs/guides/testing.md): Infrastructure testing with TestContainers
