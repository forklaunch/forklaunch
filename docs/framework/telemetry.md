---
title: Framework - Telemetry
category: References
description: Reference for OpenTelemetry integration in ForkLaunch.
---

## Overview

ForkLaunch provides built-in observability through OpenTelemetry integration:

- **OpenTelemetry SDK** - Industry-standard telemetry collection
- **Automatic instrumentation** - HTTP requests, database calls, and custom events
- **Context-aware logging** - Automatic correlation IDs and request tracing
- **Metrics API** - Counter, Gauge, Histogram, and UpDownCounter support

Integrates seamlessly with [Error Handling](/docs/framework/error-handling.md) correlation IDs and [Authorization](/docs/framework/authorization.md) events.

## Default Setup

ForkLaunch integrates OpenTelemetry for observability. Your application can send:

- **Traces** to OpenTelemetry collector (OTLP endpoint)
- **Metrics** via OpenTelemetry metrics API
- **Logs** via OpenTelemetry logs API with structured formatting

Configure the OpenTelemetry endpoint using environment variables.

## Configuration

### Environment Variables

```bash
# OpenTelemetry endpoint configuration
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318

# Service name for telemetry identification
OTEL_SERVICE_NAME=my-service

# Optional: Additional service attributes
OTEL_SERVICE_VERSION=1.0.0
OTEL_DEPLOYMENT_ENVIRONMENT=production
```

### Custom Metrics

Define custom metrics by providing a metrics definition object:

```typescript
import { OpenTelemetryCollector } from '@forklaunch/core/http';

const metricsDefinition = {
  userRegistrations: 'counter',
  activeConnections: 'gauge',
  requestDuration: 'histogram',
  queueDepth: 'upDownCounter'
} as const;

const otelCollector = new OpenTelemetryCollector(
  'my-service',
  'info',
  metricsDefinition
);

// Record metrics
otelCollector.recordMetric('userRegistrations', 1);
otelCollector.recordMetric('activeConnections', 42);
otelCollector.recordMetric('requestDuration', 125);
```

## Framework Integration

### Automatic HTTP Instrumentation

```typescript
// All HTTP endpoints are automatically instrumented
app.get(
  '/users/:id',
  {
    name: 'Get User'
    // Traces include: request/response, duration, errors
    // Metrics include: request count, response codes, latency
  },
  handler
);
```

### Authorization Events

```typescript
// Authorization failures are automatically logged and traced
const contractDetails = {
  auth: {
    method: 'jwt'
    // Auth events appear in traces and logs with correlation IDs
  }
};
```

### Error Correlation

```typescript
// Errors are automatically linked to traces via correlation IDs
throw new Error('Database connection failed');
// Shows up in: traces, logs, and metrics with full context
```

## Logging Methods

The `OpenTelemetryCollector` provides structured logging methods:

```typescript
// Log levels
otelCollector.trace('Detailed trace information');
otelCollector.debug('Debug information');
otelCollector.info('Informational message');
otelCollector.warn('Warning message');
otelCollector.error('Error message');
otelCollector.fatal('Fatal error message');

// Logging with metadata
otelCollector.info('User login', {
  userId: '123',
  correlationId: 'abc-def'
});
```

## Best Practices

1. **Use semantic naming for custom metrics** - Follow OpenTelemetry conventions
2. **Add business-relevant metrics to track KPIs** - Beyond technical metrics
3. **Keep dashboard queries efficient** - Avoid expensive aggregations
4. **Set appropriate alert thresholds** - Based on SLA requirements
5. **Leverage correlation IDs** - Connect logs, traces, and metrics
6. **Monitor resource usage** - Set up alerts for memory/CPU thresholds

## Production Considerations

### Performance Impact

- Telemetry adds ~1-3% overhead in typical applications
- Sampling can be configured to reduce trace volume
- Metrics collection is lightweight and always-on

### Data Retention

```yaml
# Configure retention in prometheus.yaml
global:
  retention: 30d # Adjust based on storage capacity
```

## Default Instrumentation

ForkLaunch automatically instruments:

- **HTTP requests and responses** - Full request lifecycle tracing
- **Database queries** - Query performance and connection metrics
- **Cache operations** - Hit rates, latency, and error tracking
- **External service calls** - Dependency monitoring and circuit breaker status
- **Runtime metrics** - CPU, memory, garbage collection, event loop lag

## Related Documentation

- **[Error Handling](/docs/framework/error-handling.md)** - Correlation IDs and error tracking
- **[Authorization](/docs/framework/authorization.md)** - Authentication event logging
- **[HTTP Frameworks](/docs/framework/http.md)** - Automatic request instrumentation
- **[Config Injector](/docs/framework/config.md)** - Dependency injection observability
