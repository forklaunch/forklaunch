---
title: Adding Projects - Workers
category: Guides
description: Learn how to add and configure workers in your application.
---

## Add a Worker

A **worker** is a background process that handles asynchronous tasks and job processing. While services handle HTTP requests synchronously, workers process jobs in the background, like sending emails, processing images, generating reports, or handling scheduled tasks. Think of it as a dedicated employee working behind the scenes: when a user places an order, a service handles the request immediately, while a worker processes the order fulfillment, sends confirmation emails, and updates inventory asynchronously.

In ForkLaunch, workers can use different backends (database, Redis, Kafka, BullMQ) depending on your needs for durability, performance, and complexity. Each worker has both a `worker` entry point for processing jobs and a `server` entry point for health checks and management APIs.

## Getting Started

You can add a worker in two ways:
1. [Using Commands](/docs/adding-projects/workers.md#using-commands-recommended)
2. [Manual Creation with Sync](/docs/adding-projects/workers.md#manual-creation-with-sync)

### Using Commands (Recommended)

<CodeTabs type="instantiate">
  <Tab title="init">

  ```bash
  forklaunch init worker
  ```

  </Tab>
  <Tab title="add">

  ```bash
  forklaunch add worker
  ```

  </Tab>
</CodeTabs>

This creates a new worker with `worker` and `server` entry points. The worker handles asynchronous job processing, and the server provides health checks and optionally exposes management APIs. All application artifacts are automatically updated.

### Manual Creation with Sync

If you manually create a worker directory, use `forklaunch sync` to register it:

```bash
# After manually creating the worker structure
forklaunch sync worker worker-name
```

This updates artifacts (manifest, docker-compose, workspace, tsconfig) to include your manually created worker.

### Sync Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--path` | `-p` | The application path | Any valid directory path that contains /modules |
| `--prompts` | `-P` | JSON object with pre-provided answers for worker options (see init options) | JSON string |

### Init Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--type` | `-t` | The worker type to use | `database`, `redis`, `kafka`, `bullmq` |
| `--database` | `-d` | The database to use (for database workers) | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `--path` | `-p` | The application path to initialize the worker in | Any valid directory path |
| `--description` | `-D` | The description of the worker | Any string |
| `--mappers` |  | Generate mapper files for entity/DTO transformation | Flag (no value) |
| `--dryrun` | `-n` | Dry run the command | Flag (no value) |

### Worker Aliases

The `worker` command has an alias for convenience:
- `wrk`

### Worker Types

ForkLaunch supports multiple worker implementations, each optimized for different use cases:

#### Database Worker (`--type database`)
- **Use Case**: Persistent job processing with full ACID guarantees
- **Features**: Database-backed job queue, complex job relationships, full audit trail
- **Best For**: Critical jobs that need durability, complex workflows, financial transactions

#### Redis Worker (`--type redis`) 
- **Use Case**: High-performance, lightweight job processing
- **Features**: In-memory job processing, fast throughput, simple pub/sub
- **Best For**: Real-time notifications, cache invalidation, simple background tasks

#### BullMQ Worker (`--type bullmq`)
- **Use Case**: Redis-based job queue with advanced features
- **Features**: Job scheduling, retry logic, job prioritization, dashboard UI
- **Best For**: Scheduled tasks, email processing, complex retry scenarios

#### Kafka Worker (`--type kafka`)
- **Use Case**: Event streaming and distributed event processing  
- **Features**: Event streaming, topic-based routing, consumer groups
- **Best For**: Event sourcing, microservice communication, analytics pipelines

### Project Structure

A worker follows the standard ForkLaunch service structure:

```bash
.
├── registrations.ts # the dependency injector/configuration loader
├── client.ts # the client for the worker, defining the event handlers
├── constants
│   └── seed.data.ts # seed data for new entity types (only relevant for database workers)
├── controllers
│   └── newWorker.controller.ts # the controller for the worker, defining the API handlers
├── eslint.config.mjs -> ../eslint.config.mjs # symlinked from parent for consistency
├── interfaces
│   └── newWorker.interface.ts
├── mikro-orm.config.ts
├── models
│   ├── mappers
│   │   └── newWorker.mappers.ts
│   ├── persistence
│   │   ├── index.ts
│   │   └── newWorkerRecord.entity.ts
│   ├── seeder.ts
│   └── seeders
│       ├── index.ts
│       └── newWorkerRecord.seeder.ts
├── package.json
├── routes
│   └── newWorker.routes.ts
├── services
│   └── newWorker.service.ts
├── tsconfig.json
├── vitest.config.ts -> ../vitest.config.ts
└── server.ts
```

### Usage Examples

```bash
# Basic database worker
forklaunch init worker email-processor --path ./my-app/src/modules --type database --database postgresql

# Redis worker for real-time tasks
forklaunch init worker notification-worker --path ./my-app/src/modules --type redis

# BullMQ worker with advanced features
forklaunch init worker scheduled-jobs --path ./my-app/src/modules --type bullmq

# Kafka worker for event streaming
forklaunch init worker analytics-consumer --path ./my-app/src/modules --type kafka

# Custom path and description
forklaunch init worker background-tasks --path ./workers --description "Background task processor"
```

For a complete step-by-step example with services, see the [Dice Roll Example](/docs/examples/dice-roll-node-app.md).

### Next Steps

After creating a worker:
1. Define your event handlers in the client
2. Implement your worker logic in the worker file
3. Set up any necessary data models (for database workers)
4. Configure event processing patterns

For database setup, migrations, building, and running the worker, see [Local Development](/docs/local-development.md).

### Common Issues

1. **Backend Connection**: Ensure backend services (Redis, Kafka, DB) are running and accessible
2. **Environment Variables**: Verify all required variables are set correctly
3. **Concurrency**: Configure appropriate concurrency limits for your worker type
4. **Memory Management**: Monitor for memory leaks in long-running job processing
5. **Error Handling**: Implement proper retry logic and dead letter queues
6. **Scaling**: Configure worker scaling based on job queue length

### Deployment

When you're ready to deploy your worker, see [Release](/docs/cli/release.md) and [Deploy](/docs/cli/deploy.md) for information about:
- Deployment commands and workflows
- Default AWS Free Tier resources
- Cost estimates and scaling options

**Worker-Specific Deployment Notes:**
- Workers use the same compute defaults as services (256m CPU, 512Mi RAM)
- Auto-scaling is based on queue depth for workers
- Database/Redis/BullMQ workers are free-tier eligible
- Kafka workers require paid infrastructure (~$70+/month)

### Best Practices

1. **Choose the Right Type**: Select worker type based on durability and performance needs
2. **Free Tier First**: Use Database or BullMQ workers to stay in free tier
3. **Idempotent Processing**: Ensure jobs can be safely retried
4. **Error Handling**: Implement comprehensive error handling and logging
5. **Monitoring**: Track job success/failure rates and processing times
6. **Graceful Shutdown**: Handle shutdown signals properly to avoid job loss
7. **Resource Management**: Set appropriate timeouts and concurrency limits
8. **Testing**: Test with realistic job volumes and failure scenarios
9. **Documentation**: Document job types and expected data structures
10. **Scale Smart**: Monitor costs via Platform UI before upgrading resources
