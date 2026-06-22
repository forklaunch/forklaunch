# ComplianceDataService Usage Guide

## Overview

The `ComplianceDataService` provides GDPR compliance operations (erasure and export) for entities defined with `defineComplianceEntity`.

**SECURITY CRITICAL**: The service can operate in two modes:
1. **Filtered mode** (default): Respects tenant/organization filters
2. **Unfiltered mode**: Operates across ALL tenants (requires superadmin authorization)

## Basic Usage (Filtered Mode - Default)

Use this for **tenant-scoped compliance operations** where users can only access their own organization's data:

```typescript
import { ComplianceDataService } from '@forklaunch/core/services';

// In your controller/service:
export class ComplianceController {
  private complianceService: ComplianceDataService;

  constructor(orm: MikroORM, otel: OpenTelemetryCollector) {
    // Default: filters remain enabled
    this.complianceService = new ComplianceDataService(orm, otel);
  }

  async eraseUserData(req: Request, res: Response) {
    // Filters are enabled - only finds users in current tenant
    const result = await this.complianceService.erase(req.params.userId);
    
    // Check BOTH counters - erase() can anonymize OR delete records
    const totalProcessed = result.recordsDeleted + result.recordsAnonymized;
    if (totalProcessed === 0) {
      return res.status(404).json({ error: 'User not found in your organization' });
    }
    
    return res.json(result);
  }
}
```

**What happens**: Queries include tenant filters, so only data within the current tenant is accessible.

**Important**: The `EraseResult` includes TWO counters:
- `recordsDeleted` — rows hard-deleted (entities with a `delete` retention policy)
- `recordsAnonymized` — rows where PII was scrubbed but the row was kept (the default)

Always check **both** counters to determine if a user was found. Checking only `recordsDeleted` will incorrectly return 404 for users whose data was successfully anonymized but not deleted.

---

## Superadmin Usage (Unfiltered Mode)

Use this for **platform-wide GDPR operations** where a superadmin must erase data across ALL tenants:

### Step 1: Verify Authorization

```typescript
import { ComplianceDataService } from '@forklaunch/core/services';
import { ForbiddenError } from '@forklaunch/core/http';

export class SuperadminComplianceController {
  constructor(
    private orm: MikroORM,
    private otel: OpenTelemetryCollector
  ) {}

  async eraseUserDataAcrossAllTenants(req: Request, res: Response) {
    // CRITICAL: Check authorization FIRST
    if (!req.user?.isSuperAdmin) {
      throw new ForbiddenError(
        'Cross-tenant GDPR operations require superadmin role'
      );
    }

    // AUDIT LOG: Record who performed cross-tenant operation
    // NOTE: Do NOT log PII like email addresses or raw IPs in compliance operations
    this.otel.info('[Compliance] Cross-tenant erasure initiated', {
      targetUserId: req.params.userId,
      performedBy: req.user.id,
      timestamp: new Date().toISOString()
    });

    // Step 2: Create service with filters disabled
    const service = new ComplianceDataService(this.orm, this.otel, {
      disableFilters: true  // Safe because we checked authorization above
    });

    // Step 3: Perform operation
    const result = await service.erase(req.params.userId);

    // AUDIT LOG: Record result
    this.otel.info('[Compliance] Cross-tenant erasure completed', {
      targetUserId: req.params.userId,
      recordsDeleted: result.recordsDeleted,
      recordsAnonymized: result.recordsAnonymized,
      entitiesAffected: result.entitiesAffected,
      performedBy: req.user.id
    });

    return res.json(result);
  }
}
```

### Step 2: Route Protection

```typescript
// In your router:
router.delete(
  '/superadmin/compliance/erase/:userId',
  requireSuperAdmin,  // Middleware that checks req.user.isSuperAdmin
  auditLog({ action: 'gdpr.erase.cross_tenant' }),
  controller.eraseUserDataAcrossAllTenants
);
```

---

## Authorization Middleware Example

```typescript
export function requireSuperAdmin(
  req: Request,
  res: Response,
  next: NextFunction
) {
  if (!req.user) {
    return res.status(401).json({ error: 'Authentication required' });
  }

  if (!req.user.isSuperAdmin) {
    // Log unauthorized attempt
    // NOTE: Do NOT log PII like email addresses or raw IPs
    logger.warn('[Security] Unauthorized superadmin access attempt', {
      userId: req.user.id,
      path: req.path
    });

    return res.status(403).json({ 
      error: 'This operation requires superadmin privileges' 
    });
  }

  next();
}
```

---

## Understanding Filter Behavior

### Example: Tenant Filter

Let's say your app has a tenant filter configured:

```typescript
// In your ORM config:
filters: {
  tenant: {
    name: 'tenant',
    cond: { organizationId: () => getCurrentTenantId() },
    entity: ['User', 'Account', 'Order']
  }
}
```

### Scenario 1: Filtered Mode (Default)

```typescript
const service = new ComplianceDataService(orm, otel);
// OR explicitly:
const service = new ComplianceDataService(orm, otel, { disableFilters: false });

// When you call erase:
await service.erase('user-123');

// Queries run with filters:
// SELECT * FROM user WHERE id = 'user-123' AND organization_id = 'current-tenant'
// SELECT * FROM account WHERE user_id = 'user-123' AND organization_id = 'current-tenant'
```

**Result**: Only deletes data within the current tenant.

### Scenario 2: Unfiltered Mode (Superadmin)

```typescript
// After verifying superadmin authorization:
const service = new ComplianceDataService(orm, otel, { 
  disableFilters: true 
});

// When you call erase:
await service.erase('user-123');

// Queries run WITHOUT filters:
// SELECT * FROM user WHERE id = 'user-123'
// SELECT * FROM account WHERE user_id = 'user-123'
```

**Result**: Deletes data across ALL tenants.

---

## Security Checklist

Before using `disableFilters: true`, ensure:

- [ ] User is authenticated
- [ ] User has superadmin role
- [ ] Operation is logged (who, what, when)
- [ ] Result is logged (what was deleted AND anonymized)
- [ ] Authorization check happens BEFORE instantiating service
- [ ] Route is protected with authorization middleware
- [ ] Audit logs do NOT contain PII (no emails, IPs, or sensitive data)

### PII in Audit Logs

**DO NOT log personal data in compliance operation logs.** This creates a compliance paradox: you're logging PII while processing a request to delete PII.

❌ **Bad** - Logs contain PII:
```typescript
logger.info('GDPR erasure', {
  email: user.email,              // ❌ PII
  ipAddress: req.ip,              // ❌ PII
  fullName: user.name             // ❌ PII
});
```

✅ **Good** - Use IDs and non-PII identifiers:
```typescript
logger.info('GDPR erasure', {
  userId: user.id,                // ✅ ID (not PII)
  performedBy: req.user.id,       // ✅ ID (not PII)
  recordsAnonymized: result.recordsAnonymized,
  recordsDeleted: result.recordsDeleted
});
```

---

## Filter Isolation

The service uses **per-query filter control** for maximum isolation and safety:

```typescript
const service = new ComplianceDataService(orm, otel, { 
  disableFilters: true 
});

await service.erase('user-123');
// Each query inside erase() uses: em.find(Entity, where, { filters: false })
// Filters are NEVER globally disabled on the entity manager
// Other concurrent operations using the same ORM instance are unaffected
```

### How It Works

When `disableFilters: true` is set, the service passes `{ filters: false }` to **each individual query**:

```typescript
const findOptions = {
  filters: this.disableFilters ? false : undefined
};
const records = await em.find(entityClass, { [userIdField]: userId }, findOptions);
```

**Why per-query is safer than global**:
- ✅ No global state mutation
- ✅ No risk of forgetting to restore filters
- ✅ Concurrent operations are isolated
- ✅ Works correctly even if the operation throws mid-execution

**Filtered mode (default)**: `findOptions` is `{}`, so queries respect all configured filters.

**Unfiltered mode**: `findOptions` is `{ filters: false }`, bypassing tenant/soft-delete/other filters for GDPR compliance.

---

## Migration from Old Code

### Before (Manual Implementation)

```typescript
// Old workaround in forklaunch-platform:
const em = this.orm.em.fork();

try {
  // Delete entities manually, disabling filters per-query (MikroORM 7.0 approach)
  const accounts = await em.find(
    Account, 
    { user: userId },
    { filters: false }  // Disable ALL filters for this query
  );
  accounts.forEach(a => em.remove(a));
  await em.flush();
  
  // Repeat for each entity type...
  const subscriptions = await em.find(
    Subscription,
    { userId },
    { filters: false }
  );
  subscriptions.forEach(s => em.remove(s));
  await em.flush();
  
  // ... more manual deletions ...
} catch (err) {
  throw new Error(`Manual deletion failed: ${err}`);
}
```

### After (Using Framework)

```typescript
// In controller (after checking authorization):
const service = new ComplianceDataService(orm, otel, {
  disableFilters: true  // Safe - already checked req.user.isSuperAdmin
});

const result = await service.erase(userId);
// Filters automatically restored!
```

---

## Common Pitfalls

### ❌ Wrong: Creating service at module/class level

```typescript
// BAD: Service is created once with disableFilters=true
export class ComplianceController {
  private service = new ComplianceDataService(orm, otel, {
    disableFilters: true  // INSECURE: Bypasses filters for ALL requests!
  });

  async erase(req: Request) {
    // Problem: No per-request authorization check!
    return await this.service.erase(req.params.userId);
  }
}
```

### ✅ Right: Creating service per-request after authorization

```typescript
// GOOD: Service is created per-request after auth check
export class ComplianceController {
  constructor(private orm: MikroORM, private otel: OpenTelemetryCollector) {}

  async erase(req: Request) {
    // Check authorization first
    if (!req.user.isSuperAdmin) {
      throw new ForbiddenError();
    }

    // Create service after authorization
    const service = new ComplianceDataService(this.orm, this.otel, {
      disableFilters: true
    });

    return await service.erase(req.params.userId);
  }
}
```

---

## Audit Logging Example

```typescript
export class AuditedComplianceService {
  async eraseWithAudit(
    userId: string,
    performedBy: User,
    reason: string
  ): Promise<EraseResult> {
    // Log start
    await this.auditLog.create({
      action: 'compliance.erase',
      targetUserId: userId,
      performedBy: performedBy.id,
      reason,
      startedAt: new Date(),
      status: 'in_progress'
    });

    try {
      const service = new ComplianceDataService(this.orm, this.otel, {
        disableFilters: true
      });

      const result = await service.erase(userId);

      // Log success
      await this.auditLog.update({
        status: 'completed',
        recordsDeleted: result.recordsDeleted,
        recordsAnonymized: result.recordsAnonymized,
        entitiesAffected: result.entitiesAffected,
        completedAt: new Date()
      });

      return result;
    } catch (err) {
      // Log failure
      await this.auditLog.update({
        status: 'failed',
        error: String(err),
        completedAt: new Date()
      });

      throw err;
    }
  }
}
```

---

## Configuration Errors and Fail-Loud Behavior

The service follows a **fail-loud** contract: any configuration error that would result in incomplete erasure or export causes an immediate failure with a structured error.

### Missing User ID Field

If a compliance-registered entity has PII but no resolvable user ID field, the operation fails:

```typescript
// ❌ Misconfigured entity
defineComplianceEntity({
  name: 'AuditLog',
  properties: {
    pk: fp.uuid().primary().compliance('none'),
    actorId: fp.uuid().compliance('none'),  // Non-standard field name
    ipAddress: fp.string().compliance('pii')
  }
  // Missing: userIdField specification
  // 'actorId' is not in CANDIDATE_USER_FIELDS: userId, user, id, partyId, etc.
});

// Attempt erasure
const service = new ComplianceDataService(orm, otel);
await service.erase('user-123');
// ❌ Throws ComplianceEraseError with structured failures
```

**Error message**:
```
ComplianceEraseError: Erase aborted: one or more entities could not be erased
failures: [
  {
    entityName: 'AuditLog',
    error: 'No user-linking field found. Entity has PII but cannot be linked to users. 
            Candidates tried: userId, user, id, partyId, customerId, ownerId, createdBy, email. 
            Fix: specify userIdField in defineComplianceEntity() or constructor options.'
  }
]
```

### How to Fix

**Option 1**: Specify `userIdField` in entity definition:
```typescript
defineComplianceEntity({
  name: 'AuditLog',
  properties: {
    pk: fp.uuid().primary().compliance('none'),
    actorId: fp.uuid().compliance('none'),
    ipAddress: fp.string().compliance('pii')
  },
  userIdField: 'actorId'  // ✅ Explicitly specify the linking field
});
```

**Option 2**: Override in service constructor:
```typescript
const service = new ComplianceDataService(orm, otel, {
  userIdFieldOverrides: {
    AuditLog: 'actorId'  // ✅ Override per entity
  }
});
```

**Option 3**: Use a standard field name that's automatically detected:
```typescript
defineComplianceEntity({
  name: 'AuditLog',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    userId: fp.uuid().compliance('none'),  // ✅ Standard name, auto-detected
    ipAddress: fp.string().compliance('pii')
  }
  // No userIdField needed - 'userId' is in CANDIDATE_USER_FIELDS
});
```

### Why Fail-Loud?

**Without fail-loud** (old behavior):
```typescript
await service.erase('user-123');
// ✅ Returns success
// ❌ But AuditLog PII was silently skipped!
// 🚨 GDPR compliance violation - incomplete erasure reported as complete
```

**With fail-loud** (current behavior):
```typescript
await service.erase('user-123');
// ❌ Throws ComplianceEraseError immediately
// ✅ Transaction rolled back - no partial erasure
// ✅ Clear error message with fix instructions
// ✅ Audit logs show configuration error, not successful erasure
```

**The contract**: A successful return (`EraseResult` or `ExportResult`) **guarantees** the operation fully succeeded. Any entity that couldn't be processed causes the entire operation to fail.

---

## Summary

| Mode | disableFilters | Use Case | Authorization Required |
|------|----------------|----------|------------------------|
| Filtered (default) | `false` or omitted | Tenant-scoped compliance | Organization admin |
| Unfiltered | `true` | Platform-wide GDPR | Superadmin only |

**Key principle**: Authorization happens at the **controller/handler level** BEFORE instantiating the service, not inside the service itself.
