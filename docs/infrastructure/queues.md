---
title: "Queues"
description: "Message queue systems: BullMQ for job processing and Kafka for event streaming."
category: "Infrastructure"
---

## Overview

ForkLaunch supports two message queue systems for background processing and event-driven architectures:

| System | Package | Backend | Best For |
|--------|---------|---------|----------|
| **BullMQ** | `@forklaunch/infrastructure-bullmq` | Redis | Job queues, retries, scheduled tasks, DLQ |
| **Kafka** | `@forklaunch/infrastructure-kafka` | Apache Kafka | High-throughput event streaming, multi-consumer |

BullMQ is the default for most use cases. Kafka is available when you need high-throughput event streaming with multiple consumer groups.

## BullMQ

### How It Works

BullMQ uses Redis as its backing store and provides reliable job processing with automatic retries, dead letter queues, and job scheduling.

### Creating a Worker

Use the CLI to generate a worker:

```bash
forklaunch init worker deployment-processor --type bullmq
```

This generates a service/worker pair with the queue configuration, job definitions, and processor logic.

### Job Definition

Jobs are defined with typed payloads and processing logic:

```typescript
interface DeploymentJob {
  applicationId: string;
  environmentId: string;
  commitHash: string;
  triggeredBy: string;
}

// Enqueue a job
await queue.add('deploy', {
  applicationId: 'app-123',
  environmentId: 'env-456',
  commitHash: 'abc123',
  triggeredBy: 'user@example.com'
}, {
  attempts: 3,
  backoff: { type: 'exponential', delay: 5000 }
});
```

### Retry and DLQ

BullMQ handles retries automatically based on your configuration:

```typescript
{
  attempts: 3,              // Retry up to 3 times
  backoff: {
    type: 'exponential',    // Exponential backoff
    delay: 5000             // Starting at 5 seconds
  },
  removeOnComplete: true,   // Clean up completed jobs
  removeOnFail: false       // Keep failed jobs for inspection
}
```

Failed jobs that exhaust all retries are moved to the dead letter queue for manual inspection and replay.

### Worker Processing

Workers process jobs from the queue:

```typescript
const worker = new Worker('deployment', async (job) => {
  const { applicationId, environmentId, commitHash } = job.data;

  // Step 1: Build
  await job.updateProgress(10);
  await buildService.build(applicationId, commitHash);

  // Step 2: Deploy
  await job.updateProgress(50);
  await deployService.deploy(environmentId);

  // Step 3: Verify
  await job.updateProgress(90);
  await healthCheckService.verify(environmentId);

  return { status: 'deployed', timestamp: Date.now() };
}, {
  connection: { url: process.env.REDIS_URL },
  concurrency: 5
});
```

## Kafka

### When to Use Kafka

Choose Kafka over BullMQ when you need:

- **Multiple consumer groups** processing the same events
- **Event sourcing** or audit log patterns
- **High-throughput** message processing (100k+ messages/second)
- **Message replay** from a specific offset

### Setup

Add Kafka when initializing your application:

```bash
forklaunch init worker event-processor --type kafka
```

### Producer

```typescript
import { KafkaProducer } from '@forklaunch/infrastructure-kafka';

const producer = new KafkaProducer({
  brokers: [process.env.KAFKA_BROKERS],
  clientId: 'my-service'
});

await producer.send({
  topic: 'deployment-events',
  messages: [{
    key: applicationId,
    value: JSON.stringify({
      type: 'DEPLOYMENT_STARTED',
      applicationId,
      environmentId,
      timestamp: Date.now()
    })
  }]
});
```

### Consumer

```typescript
import { KafkaConsumer } from '@forklaunch/infrastructure-kafka';

const consumer = new KafkaConsumer({
  brokers: [process.env.KAFKA_BROKERS],
  groupId: 'deployment-tracker'
});

await consumer.subscribe({ topic: 'deployment-events' });

await consumer.run({
  eachMessage: async ({ topic, partition, message }) => {
    const event = JSON.parse(message.value.toString());
    switch (event.type) {
      case 'DEPLOYMENT_STARTED':
        await trackDeploymentStart(event);
        break;
      case 'DEPLOYMENT_COMPLETED':
        await trackDeploymentComplete(event);
        break;
    }
  }
});
```

## Environment Variables

```bash
# BullMQ (uses Redis)
REDIS_URL=redis://localhost:6379

# Kafka
KAFKA_BROKERS=localhost:9092
KAFKA_CLIENT_ID=my-service
KAFKA_GROUP_ID=my-consumer-group
```

## Testing

Both queue systems can be tested with TestContainers:

```typescript
import { BlueprintTestHarness } from '@forklaunch/testing';

// BullMQ (needs Redis)
const harness = new BlueprintTestHarness({
  needsRedis: true
});

// Kafka
const harness = new BlueprintTestHarness({
  needsKafka: true
});

const setup = await harness.setup();
```

## Choosing Between BullMQ and Kafka

| Consideration | BullMQ | Kafka |
|---------------|--------|-------|
| Setup complexity | Low (just Redis) | Higher (Kafka cluster) |
| Job retries | Built-in with backoff | Manual implementation |
| Dead letter queue | Built-in | Manual implementation |
| Job scheduling | Built-in (cron, delayed) | Not built-in |
| Consumer groups | Single consumer | Multiple consumer groups |
| Message replay | No | Yes (offset-based) |
| Throughput | Moderate | Very high |
| AWS equivalent | ElastiCache (Redis) | Amazon MSK |

For most ForkLaunch applications, **BullMQ is the recommended default**. Use Kafka when your architecture specifically requires event streaming or multi-consumer patterns.

## Related Documentation

- [Infrastructure Overview](/docs/infrastructure/overview.md)
- [Caches](/docs/infrastructure/caches.md): Redis, which backs BullMQ
- [Testing Guide](/docs/guides/testing.md): Testing with Redis and Kafka containers
