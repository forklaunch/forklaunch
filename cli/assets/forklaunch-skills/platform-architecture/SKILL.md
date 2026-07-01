---
name: platform-architecture
description: "Architecture: modules, DDD, deployment workflow, Pulumi, multi-tenancy, worker queues."
user-invokable: true
---

# ForkLaunch Platform Architecture Skill

## When to Use This Skill

Use this skill when working with the Forklaunch Platform codebase specifically:

- Understanding the platform's service architecture
- Working with deployment infrastructure
- Implementing Pulumi-based infrastructure generation
- Managing platform resources (applications, services, environments)
- Integrating with the deployment agent worker
- Understanding the multi-module architecture

## Platform Overview

The Forklaunch Platform is a deployment and infrastructure management system that provisions AWS resources using Pulumi. It consists of multiple services and workers that handle application lifecycle management.

## Module Architecture

### Core Modules

#### 1. **core** (Library)

Shared foundational infrastructure and utilities used across all services.

**Key Components:**

- Base classes and interfaces
- Common utilities
- Shared types
- Configuration management

**Location:** `src/modules/core/`

#### 2. **monitoring** (Library)

Metrics, logs, and trace building blocks for observability.

**Key Components:**

- OpenTelemetry integration
- Logging utilities
- Metrics collection
- Trace management

**Location:** `src/modules/monitoring/`

#### 3. **universal-sdk** (Library)

Type-safe SDK for consuming platform APIs.

**Key Components:**

- Auto-generated API clients
- Type definitions
- Request/response utilities

**Location:** `src/modules/universal-sdk/`

### Service Modules

#### 1. **platform-management** (Service)

Core service managing applications, services, deployments, and infrastructure.

**Key Responsibilities:**

- Application lifecycle management
- Service and worker management
- Deployment orchestration
- Environment management
- Infrastructure resource tracking

**Routers:**

- `application`: Application CRUD operations
- `controller`: Controller management
- `deployment`: Deployment lifecycle
- `eject`: Pulumi code ejection
- `environment`: Environment management
- `instance-size`: EC2 instance sizing
- `release`: Release management
- `resource`: Infrastructure resources
- `route`: Route management
- `service`: Service management
- `worker`: Worker management

**Database:** PostgreSQL + Redis
**Location:** `src/modules/platform-management/`

#### 2. **iam** (Service)

Identity and access management service.

**Key Responsibilities:**

- Authentication (Better Auth integration)
- User management
- Organization management
- Role-based access control (RBAC)
- Permission management
- Billing integration

**Routers:**

- `auth`: Authentication endpoints
- `organization`: Organization CRUD
- `organization-management`: Org admin operations
- `user`: User management
- `role`: Role management
- `permission`: Permission management
- `billing`: Billing integration

**Database:** PostgreSQL
**Variant:** `iam-better-auth`
**Location:** `src/modules/iam/`

#### 3. **billing** (Service)

Stripe-based billing and subscription management.

**Key Responsibilities:**

- Subscription management
- Payment processing
- Plan management
- Billing portal access
- Webhook handling

**Routers:**

- `plan`: Pricing plan management
- `subscription`: Subscription lifecycle
- `billingPortal`: Customer portal access
- `checkoutSession`: Checkout flow
- `paymentLink`: Payment link generation
- `webhook`: Stripe webhook handling

**Database:** PostgreSQL + Redis
**Variant:** `billing-stripe`
**Location:** `src/modules/billing/`

### Worker Modules

#### **deployment-agent-worker** (Worker)

Orchestrates deployments and infrastructure provisioning using Pulumi.

**Key Responsibilities:**

- Deployment processing
- Pulumi infrastructure generation
- Rollback operations
- Dead letter queue (DLQ) handling
- Checkpoint management

**Routers:**

- `deployment`: Main deployment jobs
- `rollback`: Rollback operations
- `dlq`: Failed job recovery
- `deployment-agent-worker`: Worker metadata

**Infrastructure:** BullMQ (Redis-backed queue)
**Location:** `src/modules/deployment-agent-worker/`

**Key Services:**

- `PulumiGeneratorService`: Generates TypeScript Pulumi code
- `PulumiExecutorService`: Executes Pulumi operations
- `DeploymentProcessorService`: Orchestrates deployment flow
- `RollbackProcessorService`: Handles rollback logic
- `CheckpointProcessorService`: Manages deployment checkpoints

## Domain-Driven Design Patterns

The platform follows DDD principles with a layered architecture:

### Layer Structure

```
api/
├── controllers/      # HTTP request handlers
├── routes/          # Route definitions
└── middleware/      # API-specific middleware

domain/
├── services/        # Business logic layer
├── schemas/         # Validation schemas (Zod)
├── types/           # TypeScript type definitions
└── utils/           # Domain utilities

persistence/
├── entities/        # MikroORM entities
├── repositories/    # Data access layer
└── migrations/      # Database migrations
```

### Controller Pattern

Controllers handle HTTP requests and delegate to services:

```typescript
// api/controllers/deployment.controller.ts
import { injectable } from "tsyringe";
import { Request, Response } from "express";
import { DeploymentService } from "../../domain/services/deployment.service";

@injectable()
export class DeploymentController {
  constructor(private deploymentService: DeploymentService) {}

  async createDeployment(req: Request, res: Response) {
    try {
      const deployment = await this.deploymentService.create(req.body);
      return res.status(201).json(deployment);
    } catch (error) {
      return res.status(500).json({ error: error.message });
    }
  }
}
```

### Service Pattern

Services contain business logic and orchestrate operations:

```typescript
// domain/services/deployment.service.ts
import { injectable } from "tsyringe";
import { DeploymentRepository } from "../../persistence/repositories/deployment.repository";
import { QueueService } from "./queue.service";

@injectable()
export class DeploymentService {
  constructor(
    private deploymentRepository: DeploymentRepository,
    private queueService: QueueService,
  ) {}

  async create(data: CreateDeploymentDto): Promise<Deployment> {
    // Business logic
    const deployment = await this.deploymentRepository.create(data);

    // Queue deployment job
    await this.queueService.addJob("deployment", {
      deploymentId: deployment.id,
    });

    return deployment;
  }
}
```

### Entity Pattern

Entities are MikroORM models:

```typescript
// persistence/entities/deployment.entity.ts
import { Entity, PrimaryKey, Property, ManyToOne } from "@mikro-orm/core";
import { v4 } from "uuid";

@Entity()
export class Deployment {
  @PrimaryKey()
  id: string = v4();

  @Property()
  status: "pending" | "in_progress" | "completed" | "failed";

  @ManyToOne(() => Application)
  application: Application;

  @Property()
  createdAt: Date = new Date();

  @Property({ onUpdate: () => new Date() })
  updatedAt: Date = new Date();
}
```

## Pulumi Infrastructure Generation

### Pulumi Generator Service

The `PulumiGeneratorService` generates TypeScript Pulumi code from application manifests:

**Key Method:**

```typescript
async generatePulumiCode(
  application: Application,
  environment: Environment,
  services: Service[],
  workers: Worker[],
  resources: Resource[]
): Promise<string>
```

**Generated Infrastructure:**

- VPC and networking (subnets, security groups, NAT gateways)
- ECS clusters and services
- Load balancers (ALB) and target groups
- RDS databases (PostgreSQL, MySQL)
- ElastiCache clusters (Redis)
- MSK clusters (Kafka)
- ECR repositories
- CloudWatch log groups
- IAM roles and policies
- VPC endpoints (S3, ECR, ECS, Secrets Manager, etc.)
- Service Discovery (AWS Cloud Map)
- Auto-scaling policies
- DNS firewall rules
- CloudTrail audit logging

**Location:** `src/modules/deployment-agent-worker/domain/services/pulumi-generator.service.ts`

### Pulumi Executor Service

The `PulumiExecutorService` executes Pulumi operations:

**Key Methods:**

- `up()`: Deploy infrastructure
- `preview()`: Preview changes
- `destroy()`: Destroy infrastructure
- `refresh()`: Refresh state

**Features:**

- Streams logs to frontend via WebSocket
- Manages Pulumi state in S3
- Handles errors and retries
- Tracks deployment checkpoints

**Location:** `src/modules/deployment-agent-worker/domain/services/pulumi-executor.service.ts`

## Deployment Workflow

### 1. User Initiates Deployment

```
Frontend → platform-management API → Create Deployment Record
```

### 2. Queue Deployment Job

```
platform-management → BullMQ → deployment-agent-worker
```

### 3. Process Deployment

```
deployment-agent-worker:
  1. Fetch application, services, resources from DB
  2. Generate Pulumi TypeScript code
  3. Write code to temporary directory
  4. Execute Pulumi up with streaming logs
  5. Update deployment status
  6. Clean up temporary files
```

### 4. Stream Logs

```
Pulumi Executor → WebSocket → Frontend (real-time logs)
```

### 5. Complete Deployment

```
Update deployment status to "completed" or "failed"
```

## Database Patterns

### Entity Relationships

```
Organization
  ├── Applications (1:N)
  │   ├── Environments (1:N)
  │   │   ├── Deployments (1:N)
  │   │   └── EnvironmentVariables (1:N)
  │   ├── Services (1:N)
  │   ├── Workers (1:N)
  │   └── Resources (1:N)
  └── Users (N:M via OrganizationMembership)
```

### Migration Pattern

Always create migrations for schema changes:

```bash
# Create migration
pnpm migration:create --name add_deployment_status

# Apply migrations
pnpm migration:up

# Rollback
pnpm migration:down
```

## Environment Configuration

### Environment Variables

Platform services use environment variables for configuration:

```bash
# Database
DATABASE_URL=postgresql://user:pass@localhost:5432/platform

# Redis
REDIS_URL=redis://localhost:6379

# AWS
AWS_REGION=us-east-1
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...

# Platform
PLATFORM_API_URL=http://localhost:8000
FRONTEND_URL=http://localhost:3000

# Authentication
JWT_SECRET=...
SESSION_SECRET=...

# Stripe (for billing)
STRIPE_SECRET_KEY=...
STRIPE_WEBHOOK_SECRET=...
```

### Multi-Environment Strategy

```
.env.local          # Local development (not committed)
.env.development    # Development environment
.env.staging        # Staging environment
.env.production     # Production environment
```

## API Design Patterns

### RESTful Conventions

Follow REST conventions:

- `GET /api/applications` - List applications
- `GET /api/applications/:id` - Get application
- `POST /api/applications` - Create application
- `PUT /api/applications/:id` - Update application
- `DELETE /api/applications/:id` - Delete application

### Response Format

Standardized response format:

```typescript
// Success response
{
  data: { ... },
  message: "Success message"
}

// Error response
{
  error: "Error message",
  details: ["Validation error 1", "Validation error 2"],
  code: "ERROR_CODE"
}

// Paginated response
{
  data: [...],
  pagination: {
    page: 1,
    limit: 10,
    total: 100,
    totalPages: 10
  }
}
```

### Validation Pattern

Use Zod schemas for request validation:

```typescript
// domain/schemas/deployment.schema.ts
import { z } from "zod";

export const CreateDeploymentSchema = z.object({
  applicationId: z.string().uuid(),
  environmentId: z.string().uuid(),
  releaseId: z.string().uuid(),
});

export type CreateDeploymentDto = z.infer<typeof CreateDeploymentSchema>;
```

## Worker Queue Patterns

### BullMQ Job Definition

```typescript
// Register job processor
queueService.process("deployment", async (job) => {
  const { deploymentId } = job.data;

  // Process deployment
  await deploymentProcessorService.process(deploymentId);

  return { success: true };
});
```

### Job Options

```typescript
// Add job with options
await queueService.addJob("deployment", data, {
  attempts: 3, // Retry 3 times
  backoff: {
    type: "exponential",
    delay: 2000,
  },
  removeOnComplete: 100, // Keep last 100 completed jobs
  removeOnFail: false, // Keep failed jobs for debugging
});
```

### Dead Letter Queue (DLQ)

Failed jobs are moved to DLQ for manual review:

```typescript
// DLQ router handles failed jobs
forklaunchApplication.get("/api/dlq", async (req, res) => {
  const failedJobs = await queueService.getFailedJobs();
  res.json({ data: failedJobs });
});

forklaunchApplication.post("/api/dlq/:jobId/retry", async (req, res) => {
  await queueService.retryJob(req.params.jobId);
  res.json({ message: "Job queued for retry" });
});
```

## Security Patterns

### Authentication Flow

1. User authenticates via `iam` service (Better Auth)
2. JWT token issued with user ID and organization ID
3. Token sent in `Authorization: Bearer <token>` header
4. Each request validates token and extracts user context

### Authorization Pattern

```typescript
// Middleware checks permissions
const requirePermission = (permission: string) => {
  return async (req, res, next) => {
    const user = req.user;
    const hasPermission = await permissionService.check(user.id, permission);

    if (!hasPermission) {
      return res.status(403).json({ error: "Forbidden" });
    }

    next();
  };
};

// Apply to routes
forklaunchApplication.delete(
  "/api/applications/:id",
  requirePermission("application:delete"),
  deleteApplicationHandler,
);
```

### Multi-Tenancy

All data is scoped by organization:

```typescript
// Always filter by organization
const applications = await applicationRepository.find({
  organizationId: req.user.organizationId,
});

// Never allow cross-organization access
if (application.organizationId !== req.user.organizationId) {
  return res.status(403).json({ error: "Forbidden" });
}
```

## Observability

### Logging

Use structured logging:

```typescript
import { Logger } from "@modules/core";

logger.info("Deployment started", {
  deploymentId,
  applicationId,
  userId: req.user.id,
});

logger.error("Deployment failed", {
  deploymentId,
  error: error.message,
  stack: error.stack,
});
```

### Metrics

Track key metrics:

- Deployment duration
- Success/failure rates
- API response times
- Queue job processing times

### Tracing

OpenTelemetry automatically traces:

- HTTP requests
- Database queries
- Queue jobs
- External API calls

## Testing Patterns

### Unit Tests

Test services in isolation:

```typescript
// domain/services/__test__/deployment.service.test.ts
describe("DeploymentService", () => {
  let service: DeploymentService;
  let mockRepository: jest.Mocked<DeploymentRepository>;

  beforeEach(() => {
    mockRepository = {
      create: jest.fn(),
      findById: jest.fn(),
    } as any;

    service = new DeploymentService(mockRepository);
  });

  it("should create deployment", async () => {
    const data = { applicationId: "123", environmentId: "456" };
    mockRepository.create.mockResolvedValue({ id: "dep-1", ...data });

    const result = await service.create(data);

    expect(result.id).toBe("dep-1");
    expect(mockRepository.create).toHaveBeenCalledWith(data);
  });
});
```

### Integration Tests

Test full request/response cycle:

```typescript
// api/controllers/__test__/deployment.controller.test.ts
import request from "supertest";
import { app } from "../../../server";

describe("DeploymentController", () => {
  it("should create deployment", async () => {
    const response = await request(app)
      .post("/api/deployments")
      .send({
        applicationId: "123",
        environmentId: "456",
        releaseId: "789",
      })
      .expect(201);

    expect(response.body.data).toHaveProperty("id");
  });
});
```

## Best Practices

1. **Follow DDD Layers**: Keep controllers thin, business logic in services
2. **Use Dependency Injection**: Always use `@injectable()` and constructor injection
3. **Validate Early**: Validate at API boundary with Zod schemas
4. **Type Everything**: Leverage TypeScript for type safety
5. **Handle Errors Gracefully**: Use try-catch and return appropriate status codes
6. **Log Strategically**: Log important events with context
7. **Test Thoroughly**: Unit test services, integration test APIs
8. **Document APIs**: Use Forklaunch metadata for OpenAPI generation
9. **Secure by Default**: Always check permissions and organization scope
10. **Monitor Everything**: Use observability for insights

## When Claude Code Should Use This Skill

1. Adding new features to platform services
2. Implementing deployment workflows
3. Working with Pulumi infrastructure generation
4. Adding API endpoints to platform-management or iam
5. Implementing worker jobs in deployment-agent-worker
6. Understanding the module architecture
7. Following platform-specific patterns and conventions
8. Implementing multi-tenancy and authorization
