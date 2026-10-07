---
title: Changing Workers
category: Changing Projects
description: Complete guide for modifying ForkLaunch worker types, queue systems, and processing configurations.
---

## Overview

The `forklaunch change worker` command allows you to modify existing worker configuration including worker types, queue systems, database connections, and metadata. This guide covers all available options and migration strategies.

```bash
forklaunch change worker [worker-name] [options]
```

If no worker name is provided, you'll be prompted to select from available workers.

## Available Options

| Option                  | Type                           | Description               | Default             |
| ----------------------- | ------------------------------ | ------------------------- | ------------------- |
| `--name <name>`         | string                         | Change worker name        | Current name        |
| `--description <desc>`  | string                         | Update worker description | Current description |
| `--type <type>`         | database\|redis\|kafka\|bullmq | Change worker type        | Current type        |
| `--database <database>` | See database options           | Change database type      | Current database    |

## Worker Types

Available worker types:

- `database` - Database-driven worker for polling and processing
- `redis` - Redis-based pub/sub worker
- `kafka` - Apache Kafka consumer worker
- `bullmq` - Bull MQ queue worker with advanced features

## Database Options

For database workers:

- `postgresql` - PostgreSQL database
- `mysql` - MySQL database
- `mariadb` - MariaDB database
- `mssql` - Microsoft SQL Server
- `mongodb` - MongoDB database
- `libsql` - LibSQL (SQLite-compatible)
- `sqlite` - SQLite database
- `better-sqlite` - Better SQLite3

## Batch Changes

### Complete Worker Modernization

Upgrade worker type and add enhanced features:

```bash
forklaunch change worker email-processor \
  --type bullmq \
  --description "High-performance email processing with BullMQ"
```

### Multiple Workers Migration

Update multiple workers to same configuration:

```bash
# Migrate all workers to BullMQ
forklaunch change worker email-processor --type bullmq
forklaunch change worker sms-processor --type bullmq
forklaunch change worker push-processor --type bullmq
```

## Migration Scenarios

### Scenario 1: Scale Up Processing

Migrate from simple database polling to high-performance queue:

```bash
# Phase 1: Migrate to Redis for better performance
forklaunch change worker batch-processor --type redis

# Phase 2: Upgrade to BullMQ for advanced features
forklaunch change worker batch-processor --type bullmq
```

### Scenario 2: Event-Driven Architecture

Migrate to Kafka for event-driven processing:

```bash
# Migrate analytics workers to Kafka
forklaunch change worker user-analytics --type kafka
forklaunch change worker order-analytics --type kafka
forklaunch change worker system-analytics --type kafka
```

### Scenario 3: Simplify Development

Use database workers for simpler local development:

```bash
# Simplify for development
forklaunch change worker email-processor --type database --database sqlite
```

## Troubleshooting

### Queue Connection Issues

**1. Redis Connection Problems**

```bash
# Test Redis connectivity
redis-cli ping

# Check Redis logs
docker logs redis-container
```

**2. Kafka Consumer Issues**

```bash
# Check consumer group status
kafka-consumer-groups.sh --bootstrap-server localhost:9092 --describe --group order-processor

# Reset consumer offset
kafka-consumer-groups.sh --bootstrap-server localhost:9092 --reset-offsets --group order-processor --topic order-events --to-earliest --execute
```

## Best Practices

### Worker Design Patterns

1. **Idempotent Processing**: Ensure jobs can be safely retried
2. **Error Handling**: Implement proper error recovery
3. **Progress Tracking**: Update job progress for long-running tasks
4. **Resource Management**: Properly close connections and clean up resources
5. **Monitoring**: Add logging and metrics for observability

### Performance Optimization

1. **Batch Processing**: Process multiple items together when possible
2. **Connection Pooling**: Use connection pools for database workers
3. **Concurrency Control**: Balance throughput with resource usage
4. **Queue Management**: Configure appropriate retention policies

### Testing Strategies

1. **Unit Tests**: Test individual worker methods
2. **Integration Tests**: Test with actual queue systems
3. **Load Tests**: Verify performance under load
4. **Error Scenarios**: Test failure and recovery paths

## Related Documentation

- **[Changing Services](./services)** - Service configuration changes
- **[Adding Workers](../adding-projects/workers)** - Creating new workers
- **[Framework Workers](../framework/workers)** - Worker framework documentation
