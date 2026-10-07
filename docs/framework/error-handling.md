---
title: Framework - Error Handling
category: References
description: Reference for Error Handling in ForkLaunch.
---

## Overview

ForkLaunch provides standardized error handling with automatic:

- **HTTP status code mapping** - Consistent error responses across all endpoints
- **Correlation ID tracking** - Links errors to specific requests for debugging
- **Telemetry integration** - Automatic error logging and metrics collection
- **Security-first approach** - Prevents sensitive data exposure in production

Integrated with [Authorization](/docs/framework/authorization.md), [HTTP Frameworks](/docs/framework/http.md), and [Telemetry](/docs/framework/telemetry.md) for comprehensive error management.

## Standard Error Responses

ForkLaunch uses Express-style error handling with a default handler that returns a 500 status code and correlation ID, hiding internal error details.

### Authentication Errors

- **401 Unauthorized**: Missing or invalid credentials
- **403 Forbidden**: Valid credentials but insufficient permissions

### Validation Errors

- **400 Bad Request**: Invalid input data that fails [validation](/docs/framework/validation.md) schemas

### Not Found (404)

Resource not found errors with path information

### Server Errors (500)

Unexpected errors with safe error messages

## Framework Integration

### HTTP Contract Error Handling

```typescript
// Errors are automatically handled and formatted
app.post(
  '/users',
  {
    name: 'Create User',
    body: { name: string, email: email }, // Validation errors → 400
    responses: {
      201: { id: number },
      // override default error responses
      400: { error: string }, // Validation failure
      401: { error: string }, // Auth required
      500: { error: string, correlationId: string }
    }
  },
  handler
);
```

### Authorization Integration

```typescript
// Authorization errors are handled automatically
const contractDetails = {
  auth: {
    method: 'jwt',
    allowedRoles: new Set(['admin'])
  }
  // 401/403 responses added automatically
};
```

## Correlation IDs

Every workflow gets a unique correlation ID that:

- **Links related logs, metrics, and traces** across services
- **Helps with request tracking** through the entire request lifecycle
- **Simplifies debugging and support** with traceable request context
- **Integrates with telemetry** for comprehensive observability

## Error Logging

Errors are automatically:

- **Logged with appropriate severity** levels based on error type
- **Tagged with correlation IDs** for request correlation
- **Linked to related telemetry** including traces and metrics
- **Filtered for security** to prevent sensitive data exposure

## Security Considerations

1. **Never expose internal error details in production** - Use safe error messages
2. **Log full error details for debugging** - Internal logs can contain sensitive data
3. **Use appropriate error status codes** - Follow HTTP standards for proper client handling
4. **Validate all error response data** - Ensure error responses don't leak information
5. **Include retry-after headers when appropriate** - Guide client retry behavior

## Response Status Codes

| Code | Description           | Usage                                   |
| ---- | --------------------- | --------------------------------------- |
| 400  | Bad Request           | Invalid input, validation errors        |
| 401  | Unauthorized          | Missing or invalid authentication       |
| 403  | Forbidden             | Valid auth but insufficient permissions |
| 404  | Not Found             | Resource doesn't exist                  |
| 429  | Too Many Requests     | Rate limit exceeded (coming soon)       |
| 500  | Internal Server Error | Unexpected server errors                |
| 503  | Service Unavailable   | Service temporarily unavailable         |

## Related Documentation

- **[Authorization](/docs/framework/authorization.md)** - Authentication and permission error handling
- **[HTTP Frameworks](/docs/framework/http.md)** - ContractDetails error response definitions
- **[Validation](/docs/framework/validation.md)** - Input validation and 400 error generation
- **[Telemetry](/docs/framework/telemetry.md)** - Error logging, metrics, and tracing
