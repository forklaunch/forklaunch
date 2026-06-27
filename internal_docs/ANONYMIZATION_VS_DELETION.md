# GDPR Erasure: Anonymization vs Deletion

**Critical for consuming repos**: Understanding when records are anonymized vs deleted.

---

## The Problem

When erasing user data for GDPR compliance, you can't always **hard delete** records because of:
- **Foreign key constraints** - Other entities reference this record
- **Audit requirements** - Need to maintain existence proof for compliance
- **System integrity** - Deletion would break relationships

**Solution**: **Anonymize** (tombstone) instead of delete.

---

## How It Works

### Default Behavior: ANONYMIZE

If you don't specify a retention policy, entities are **anonymized**:

```typescript
const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().compliance('pii'),
    name: fp.string().compliance('pii')
  }
  // No retention policy → defaults to anonymize
});

// When erasing:
// BEFORE: { id: 'user-1', email: 'alice@example.com', name: 'Alice' }
// AFTER:  { id: 'user-1', email: null, name: null }
//         ↑ Record still exists, PII nulled
```

**What happens:**
1. All PII/PHI/PCI/SOX fields → `null`
2. Record stays in database
3. Foreign keys remain intact
4. If `complianceErasedAt` field exists → set to current timestamp

---

## Explicit Deletion

Use `retention.action = 'delete'` to hard-delete records:

```typescript
const Session = defineComplianceEntity({
  name: 'Session',
  retention: {
    duration: 'P30D',
    action: 'delete'  // ← Explicit deletion
  },
  properties: {
    id: fp.uuid().primary().compliance('none'),
    userId: fp.uuid().compliance('none'),
    token: fp.string().compliance('pii'),
    createdAt: fp.datetime().compliance('none')  // Required!
  }
});

// When erasing:
// Record is removed from database entirely
```

**When to use:**
- No foreign key dependencies
- Short-lived data (sessions, tokens)
- No audit trail needed

---

## Entity Design Pattern: complianceErasedAt

Add a `complianceErasedAt` timestamp to track tombstoned records:

```typescript
const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().compliance('pii'),
    name: fp.string().compliance('pii'),
    complianceErasedAt: fp.datetime().nullable().compliance('none')
    //                  ↑ Automatically set on anonymization
  }
});

// After erasure:
// { id: 'user-1', email: null, name: null, complianceErasedAt: '2026-06-24T...' }
```

**Use this timestamp to:**
- Filter out erased users in queries
- Show "Account Deleted" in UI
- Audit compliance operations
- Track when erasure occurred

---

## Example: User Entity (Tombstone Pattern)

```typescript
// User entity - has FK dependencies (Account, Session, etc.)
const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().unique().compliance('pii'),
    name: fp.string().compliance('pii'),
    passwordHash: fp.string().compliance('phi'),
    complianceErasedAt: fp.datetime().nullable().compliance('none'),
    createdAt: fp.datetime().compliance('none')
  }
  // No retention policy → anonymize (default)
});

// After erase('user-123'):
// {
//   id: 'user-123',
//   email: null,
//   name: null,
//   passwordHash: null,
//   complianceErasedAt: '2026-06-24T12:00:00Z',
//   createdAt: '2025-01-01T00:00:00Z'
// }
```

---

## Example: Session Entity (Delete Pattern)

```typescript
// Session entity - no FK dependencies, short-lived
const Session = defineComplianceEntity({
  name: 'Session',
  retention: {
    duration: 'P30D',
    action: 'delete'  // ← Hard delete
  },
  properties: {
    id: fp.uuid().primary().compliance('none'),
    userId: fp.uuid().compliance('none'),
    token: fp.string().compliance('pii'),
    ipAddress: fp.string().compliance('pii'),
    createdAt: fp.datetime().compliance('none')
  }
});

// After erase('user-123'):
// Record is completely removed from database
```

---

## Querying Tombstoned Records

### Filter out erased users in application code:

```typescript
// Find active users only
const activeUsers = await em.find(User, {
  complianceErasedAt: null  // Not tombstoned
});

// Find erased users (for admin/audit)
const erasedUsers = await em.find(User, {
  complianceErasedAt: { $ne: null }  // Tombstoned
});
```

### Or add a global filter:

```typescript
// In ORM config
filters: {
  activeUsersOnly: {
    name: 'activeUsersOnly',
    cond: { complianceErasedAt: null },
    entity: ['User'],
    default: true  // Auto-apply to all queries
  }
}
```

---

## EraseResult Structure

```typescript
interface EraseResult {
  entitiesAffected: string[];  // ['User', 'Account', 'Session']
  recordsDeleted: number;      // Hard deletions
  recordsAnonymized: number;   // Tombstoned records
}

// Example result:
// {
//   entitiesAffected: ['User', 'Account', 'Session'],
//   recordsDeleted: 2,      // 2 sessions deleted
//   recordsAnonymized: 3    // 1 user + 2 accounts anonymized
// }
```

**Check both counters to detect 404:**

```typescript
const result = await complianceService.erase(userId);

const totalProcessed = result.recordsDeleted + result.recordsAnonymized;
if (totalProcessed === 0) {
  return res.status(404).json({ error: 'User not found' });
}

return res.json(result);
```

---

## Migration Guide for Consuming Repos

### If you have User entities with FK dependencies:

**Before** (old global registry - might have FK issues):
```typescript
// User was being deleted, causing FK violations
const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().compliance('pii')
  }
});
```

**After** (ORM metadata - anonymizes by default):
```typescript
// Same code, but now anonymizes instead of deletes
const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().compliance('pii'),
    complianceErasedAt: fp.datetime().nullable().compliance('none')  // ← Add this
  }
});
```

**No code changes needed in service layer!** The framework handles it automatically.

---

## When to Use Each Approach

### Use ANONYMIZE (default) for:
- ✅ User entities (have FK dependencies)
- ✅ Core domain entities (Account, Organization, etc.)
- ✅ Audit trail requirements
- ✅ Entities referenced by other tables

### Use DELETE (explicit) for:
- ✅ Session tokens
- ✅ Temporary records
- ✅ No FK dependencies
- ✅ Short-lived data

---

## Summary

| Aspect | Anonymize (Default) | Delete (Explicit) |
|--------|---------------------|-------------------|
| **Action** | Null PII fields | Remove row |
| **Record exists?** | Yes (tombstone) | No |
| **FK safe?** | Yes | Only if no FKs |
| **Audit trail?** | Yes | No |
| **Specify how?** | (default) | `retention: { action: 'delete' }` |
| **Best for** | User, Account, Order | Session, Token, Cache |

**Default is anonymize** - this prevents FK constraint violations and is GDPR-compliant. 🎯
