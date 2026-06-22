import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearComplianceRegistries,
  registerEntityCompliance,
  registerEntityRetention,
  registerEntityUserIdField,
  type ComplianceLevel,
  type RetentionAction
} from '../src/persistence/complianceTypes';
import {
  ComplianceDataService,
  ComplianceEraseError,
  ComplianceExportError
} from '../src/services/complianceDataService';

/**
 * Unit tests for ComplianceDataService.erase / .export.
 *
 * These drive the service against a lightweight mock MikroORM (no DB driver is
 * available in this package) plus the real compliance/retention registries, so
 * they exercise the actual action-resolution, anonymize/delete, and fail-loud
 * behavior.
 */

type FieldMeta = { nullable?: boolean };
type EntityMeta = {
  className: string;
  class?: unknown;
  properties: Record<string, FieldMeta>;
};

interface MockOrmOptions {
  /** Simulate a transaction commit failure (e.g. FK violation). */
  commitFails?: boolean;
  /** Entity name whose `find` should throw, to simulate a scan failure. */
  findThrowsFor?: string;
}

function registerEntity(
  name: string,
  fields: Record<string, ComplianceLevel>,
  opts?: {
    userIdField?: string;
    retention?: { duration: string; action: RetentionAction };
  }
): void {
  registerEntityCompliance(name, new Map(Object.entries(fields)));
  if (opts?.userIdField) {
    registerEntityUserIdField(name, opts.userIdField);
  }
  if (opts?.retention) {
    registerEntityRetention(name, opts.retention);
  }
}

/**
 * Deep clone helper for snapshotting record state before a transaction.
 * Handles nested objects and arrays but not complex types (Date, etc.).
 */
function deepClone<T>(obj: T): T {
  if (obj === null || typeof obj !== 'object') return obj;
  if (Array.isArray(obj)) return obj.map(deepClone) as T;
  const cloned: Record<string, unknown> = {};
  for (const key in obj) {
    if (Object.prototype.hasOwnProperty.call(obj, key)) {
      cloned[key] = deepClone((obj as Record<string, unknown>)[key]);
    }
  }
  return cloned as T;
}

function makeOrm(
  metas: EntityMeta[],
  recordsByEntity: Record<string, Record<string, unknown>[]>,
  options: MockOrmOptions = {}
) {
  const removed: Record<string, unknown>[] = [];

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const em: any = {};
  em.find = async (
    entityClass: unknown,
    where?: Record<string, unknown>
  ): Promise<Record<string, unknown>[]> => {
    const name =
      typeof entityClass === 'string'
        ? entityClass
        : (entityClass as { className?: string })?.className;
    if (options.findThrowsFor && name === options.findThrowsFor) {
      throw new Error(`simulated scan failure for ${name}`);
    }

    const allRecords = recordsByEntity[name as string] ?? [];

    // If no where clause provided, return all records
    if (!where || Object.keys(where).length === 0) {
      return allRecords;
    }

    // Filter records based on where clause (simple equality match)
    return allRecords.filter((record) => {
      return Object.entries(where).every(
        ([key, value]) => record[key] === value
      );
    });
  };
  em.remove = (record: Record<string, unknown>): void => {
    removed.push(record);
  };
  em.transactional = async (
    cb: (tem: unknown) => Promise<unknown>
  ): Promise<unknown> => {
    // Snapshot all record state before transaction starts
    const snapshots = new Map<
      Record<string, unknown>,
      Record<string, unknown>
    >();
    const removedSnapshot = [...removed];

    for (const records of Object.values(recordsByEntity)) {
      for (const record of records) {
        snapshots.set(record, deepClone(record));
      }
    }

    try {
      const result = await cb(em);

      // Simulate commit failure AFTER mutations have occurred
      if (options.commitFails) {
        throw new Error(
          'update or delete on table "user" violates foreign key constraint'
        );
      }

      // Commit succeeded - snapshots can be discarded
      return result;
    } catch (err) {
      // Rollback: restore all records to their pre-transaction state
      for (const [record, snapshot] of snapshots.entries()) {
        // Clear the record and restore all properties
        for (const key in record) {
          if (Object.prototype.hasOwnProperty.call(record, key)) {
            delete record[key];
          }
        }
        Object.assign(record, snapshot);
      }

      // Rollback removed array
      removed.length = 0;
      removed.push(...removedSnapshot);

      // Re-throw the error
      throw err;
    }
  };

  const orm = {
    em: { fork: () => em },
    getMetadata: () => ({
      getAll: () => new Map(metas.map((m) => [m.className, m]))
    })
  };

  return { orm, em, removed };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const otel: any = {
  warn: () => undefined,
  info: () => undefined,
  error: () => undefined
};

describe('ComplianceDataService.erase', () => {
  beforeEach(() => {
    // Clear global registries before each test to prevent cross-test pollution
    clearComplianceRegistries();
  });
  it('anonymizes (scrubs nullable PII, keeps row) by default when no retention policy is set', async () => {
    registerEntity(
      'EraseUser',
      { id: 'none', email: 'pii', name: 'pii' },
      { userIdField: 'id' }
    );

    const record = {
      id: 'u1',
      email: 'alice@example.com',
      name: 'Alice',
      complianceErasedAt: null as Date | null
    };

    const { orm, removed } = makeOrm(
      [
        {
          className: 'EraseUser',
          properties: {
            id: { nullable: false },
            email: { nullable: true },
            name: { nullable: true },
            complianceErasedAt: { nullable: true }
          }
        }
      ],
      { EraseUser: [record] }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.erase('u1');

    expect(result.recordsAnonymized).toBe(1);
    expect(result.recordsDeleted).toBe(0);
    expect(result.entitiesAffected).toContain('EraseUser');

    // PII scrubbed, structural (none) field retained, row not deleted
    expect(record.email).toBeNull();
    expect(record.name).toBeNull();
    expect(record.id).toBe('u1');
    expect(record.complianceErasedAt).toBeInstanceOf(Date);
    expect(removed).toHaveLength(0);
  });

  it('hard-deletes rows when the entity has a delete retention policy', async () => {
    registerEntity(
      'EraseSession',
      { id: 'none', token: 'pii' },
      {
        userIdField: 'userId',
        retention: { duration: 'P1D', action: 'delete' }
      }
    );

    const record = { id: 's1', userId: 'u1', token: 'secret' };
    const { orm, removed } = makeOrm(
      [
        {
          className: 'EraseSession',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            token: { nullable: true }
          }
        }
      ],
      { EraseSession: [record] }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.erase('u1');

    expect(result.recordsDeleted).toBe(1);
    expect(result.recordsAnonymized).toBe(0);
    expect(result.entitiesAffected).toContain('EraseSession');
    expect(removed).toHaveLength(1);
    expect(removed[0]).toBe(record);
  });

  it('fails loudly when an anonymize entity has a non-nullable PII field', async () => {
    registerEntity(
      'EraseProfile',
      { id: 'none', ssn: 'pci' },
      { userIdField: 'userId' }
    );

    const record = { id: 'p1', userId: 'u1', ssn: '123-45-6789' };
    const { orm, removed } = makeOrm(
      [
        {
          className: 'EraseProfile',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            ssn: { nullable: false }
          }
        }
      ],
      { EraseProfile: [record] }
    );

    const service = new ComplianceDataService(orm as never, otel);

    await expect(service.erase('u1')).rejects.toBeInstanceOf(
      ComplianceEraseError
    );

    // Nothing was scrubbed or removed
    expect(record.ssn).toBe('123-45-6789');
    expect(removed).toHaveLength(0);

    // Failure detail identifies the offending field
    try {
      await service.erase('u1');
    } catch (err) {
      const eraseErr = err as ComplianceEraseError;
      expect(eraseErr.userId).toBe('u1');
      expect(eraseErr.failures[0].entityName).toBe('EraseProfile');
      expect(eraseErr.failures[0].error).toContain('ssn');
    }
  });

  it('wraps a transaction commit failure (e.g. FK violation) as a (commit) failure', async () => {
    registerEntity(
      'EraseCommit',
      { id: 'none', email: 'pii' },
      { userIdField: 'id' }
    );

    const record = {
      id: 'u1',
      email: 'sensitive@example.com',
      complianceErasedAt: null as Date | null
    };

    const { orm, removed } = makeOrm(
      [
        {
          className: 'EraseCommit',
          properties: {
            id: { nullable: false },
            email: { nullable: true },
            complianceErasedAt: { nullable: true }
          }
        }
      ],
      { EraseCommit: [record] },
      { commitFails: true }
    );

    const service = new ComplianceDataService(orm as never, otel);

    try {
      await service.erase('u1');
      expect.unreachable('erase should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ComplianceEraseError);
      const eraseErr = err as ComplianceEraseError;
      const commitFailure = eraseErr.failures.find(
        (f) => f.entityName === '(commit)'
      );
      expect(commitFailure).toBeDefined();
      expect(commitFailure?.error).toContain('foreign key');

      // CRITICAL: Validate atomicity guarantee - transaction rollback means
      // the record should be UNCHANGED (PII still present, no erasure timestamp)
      expect(record.email).toBe('sensitive@example.com');
      expect(record.complianceErasedAt).toBeNull();
      expect(removed).toHaveLength(0);
    }
  });

  it('reports a scan failure and aborts without committing', async () => {
    registerEntity(
      'EraseScanFail',
      { id: 'none', email: 'pii' },
      { userIdField: 'id' }
    );

    const { orm, removed } = makeOrm(
      [
        {
          className: 'EraseScanFail',
          properties: { id: { nullable: false }, email: { nullable: true } }
        }
      ],
      {},
      { findThrowsFor: 'EraseScanFail' }
    );

    const service = new ComplianceDataService(orm as never, otel);

    try {
      await service.erase('u1');
      expect.unreachable('erase should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ComplianceEraseError);
      const eraseErr = err as ComplianceEraseError;
      expect(eraseErr.failures[0].entityName).toBe('EraseScanFail');
    }
    expect(removed).toHaveLength(0);
  });

  it('only erases records for the specified user (validates where-clause filtering)', async () => {
    registerEntity(
      'MultiUserEntity',
      { id: 'none', userId: 'none', email: 'pii', name: 'pii' },
      { userIdField: 'userId' }
    );

    // Records for three different users
    const user1Record1 = {
      id: 'r1',
      userId: 'user-1',
      email: 'alice@example.com',
      name: 'Alice',
      complianceErasedAt: null as Date | null
    };
    const user1Record2 = {
      id: 'r2',
      userId: 'user-1',
      email: 'alice2@example.com',
      name: 'Alice Alt',
      complianceErasedAt: null as Date | null
    };
    const user2Record = {
      id: 'r3',
      userId: 'user-2',
      email: 'bob@example.com',
      name: 'Bob',
      complianceErasedAt: null as Date | null
    };
    const user3Record = {
      id: 'r4',
      userId: 'user-3',
      email: 'charlie@example.com',
      name: 'Charlie',
      complianceErasedAt: null as Date | null
    };

    const { orm, removed } = makeOrm(
      [
        {
          className: 'MultiUserEntity',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            email: { nullable: true },
            name: { nullable: true },
            complianceErasedAt: { nullable: true }
          }
        }
      ],
      {
        MultiUserEntity: [user1Record1, user1Record2, user2Record, user3Record]
      }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.erase('user-1');

    // Should have anonymized exactly 2 records (both for user-1)
    expect(result.recordsAnonymized).toBe(2);
    expect(result.recordsDeleted).toBe(0);
    expect(result.entitiesAffected).toContain('MultiUserEntity');

    // user-1's records should be scrubbed
    expect(user1Record1.email).toBeNull();
    expect(user1Record1.name).toBeNull();
    expect(user1Record1.userId).toBe('user-1');
    expect(user1Record1.complianceErasedAt).toBeInstanceOf(Date);

    expect(user1Record2.email).toBeNull();
    expect(user1Record2.name).toBeNull();
    expect(user1Record2.userId).toBe('user-1');
    expect(user1Record2.complianceErasedAt).toBeInstanceOf(Date);

    // user-2 and user-3 records should be UNTOUCHED
    expect(user2Record.email).toBe('bob@example.com');
    expect(user2Record.name).toBe('Bob');
    expect(user2Record.complianceErasedAt).toBeNull();

    expect(user3Record.email).toBe('charlie@example.com');
    expect(user3Record.name).toBe('Charlie');
    expect(user3Record.complianceErasedAt).toBeNull();

    expect(removed).toHaveLength(0);
  });

  it('rolls back all mutations when transaction commit fails (atomicity guarantee)', async () => {
    registerEntity(
      'RollbackUser',
      { id: 'none', email: 'pii', name: 'pii' },
      { userIdField: 'id' }
    );
    registerEntity(
      'RollbackProfile',
      { id: 'none', userId: 'none', bio: 'pii', phone: 'pii' },
      { userIdField: 'userId' }
    );

    // Multiple records across multiple entities
    const userRecord = {
      id: 'user-1',
      email: 'alice@example.com',
      name: 'Alice Smith',
      complianceErasedAt: null as Date | null
    };

    const profile1 = {
      id: 'p1',
      userId: 'user-1',
      bio: 'Software engineer',
      phone: '555-0001',
      complianceErasedAt: null as Date | null
    };

    const profile2 = {
      id: 'p2',
      userId: 'user-1',
      bio: 'Data scientist',
      phone: '555-0002',
      complianceErasedAt: null as Date | null
    };

    const { orm, removed } = makeOrm(
      [
        {
          className: 'RollbackUser',
          properties: {
            id: { nullable: false },
            email: { nullable: true },
            name: { nullable: true },
            complianceErasedAt: { nullable: true }
          }
        },
        {
          className: 'RollbackProfile',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            bio: { nullable: true },
            phone: { nullable: true },
            complianceErasedAt: { nullable: true }
          }
        }
      ],
      {
        RollbackUser: [userRecord],
        RollbackProfile: [profile1, profile2]
      },
      { commitFails: true }
    );

    const service = new ComplianceDataService(orm as never, otel);

    try {
      await service.erase('user-1');
      expect.unreachable('erase should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ComplianceEraseError);

      // ALL records should be rolled back to original state
      expect(userRecord.email).toBe('alice@example.com');
      expect(userRecord.name).toBe('Alice Smith');
      expect(userRecord.complianceErasedAt).toBeNull();

      expect(profile1.bio).toBe('Software engineer');
      expect(profile1.phone).toBe('555-0001');
      expect(profile1.complianceErasedAt).toBeNull();

      expect(profile2.bio).toBe('Data scientist');
      expect(profile2.phone).toBe('555-0002');
      expect(profile2.complianceErasedAt).toBeNull();

      // No records should be in the remove queue
      expect(removed).toHaveLength(0);
    }
  });

  it('rolls back delete operations when transaction commit fails', async () => {
    registerEntity(
      'RollbackDelete',
      { id: 'none', token: 'pii' },
      {
        userIdField: 'userId',
        retention: { duration: 'P30D', action: 'delete' }
      }
    );

    const session1 = {
      id: 's1',
      userId: 'user-1',
      token: 'session-token-1'
    };
    const session2 = {
      id: 's2',
      userId: 'user-1',
      token: 'session-token-2'
    };

    const { orm, removed } = makeOrm(
      [
        {
          className: 'RollbackDelete',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            token: { nullable: true }
          }
        }
      ],
      {
        RollbackDelete: [session1, session2]
      },
      { commitFails: true }
    );

    const service = new ComplianceDataService(orm as never, otel);

    try {
      await service.erase('user-1');
      expect.unreachable('erase should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ComplianceEraseError);

      // Records should still have their original values
      expect(session1.token).toBe('session-token-1');
      expect(session2.token).toBe('session-token-2');

      // The remove queue should be rolled back (empty)
      expect(removed).toHaveLength(0);
    }
  });

  it('returns zero records when userIdField does not match any records', async () => {
    registerEntity(
      'NoMatchEntity',
      { id: 'none', ownerId: 'none', secret: 'pii' },
      { userIdField: 'ownerId' }
    );

    const { orm, removed } = makeOrm(
      [
        {
          className: 'NoMatchEntity',
          properties: {
            id: { nullable: false },
            ownerId: { nullable: false },
            secret: { nullable: true }
          }
        }
      ],
      {
        NoMatchEntity: [
          { id: 'n1', ownerId: 'user-99', secret: 'data1' },
          { id: 'n2', ownerId: 'user-88', secret: 'data2' }
        ]
      }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.erase('user-1');

    // No records should match, so nothing erased
    expect(result.recordsAnonymized).toBe(0);
    expect(result.recordsDeleted).toBe(0);
    expect(result.entitiesAffected).toEqual([]);
    expect(removed).toHaveLength(0);
  });
});

describe('ComplianceDataService.export', () => {
  beforeEach(() => {
    // Clear global registries before each test to prevent cross-test pollution
    clearComplianceRegistries();
  });

  it('exports only id + PII fields for matching records', async () => {
    registerEntity(
      'ExportAccount',
      { id: 'none', accessToken: 'pci', provider: 'none' },
      { userIdField: 'userId' }
    );

    const { orm } = makeOrm(
      [
        {
          className: 'ExportAccount',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            accessToken: { nullable: true },
            provider: { nullable: false }
          }
        }
      ],
      {
        ExportAccount: [
          { id: 'a1', userId: 'u1', accessToken: 'tok', provider: 'google' }
        ]
      }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.export('u1');

    expect(result.userId).toBe('u1');
    expect(result.entities['ExportAccount']).toHaveLength(1);
    // provider is 'none' and must be excluded; id + pci field included
    expect(result.entities['ExportAccount'][0]).toEqual({
      id: 'a1',
      accessToken: 'tok'
    });
  });

  it('throws ComplianceExportError when an entity cannot be read', async () => {
    registerEntity(
      'ExportFail',
      { id: 'none', email: 'pii' },
      { userIdField: 'id' }
    );

    const { orm } = makeOrm(
      [
        {
          className: 'ExportFail',
          properties: { id: { nullable: false }, email: { nullable: true } }
        }
      ],
      {},
      { findThrowsFor: 'ExportFail' }
    );

    const service = new ComplianceDataService(orm as never, otel);

    await expect(service.export('u1')).rejects.toBeInstanceOf(
      ComplianceExportError
    );
  });

  it('only exports records for the specified user (validates where-clause filtering)', async () => {
    registerEntity(
      'ExportMultiUser',
      { id: 'none', userId: 'none', email: 'pii', ssn: 'phi', status: 'none' },
      { userIdField: 'userId' }
    );

    const { orm } = makeOrm(
      [
        {
          className: 'ExportMultiUser',
          properties: {
            id: { nullable: false },
            userId: { nullable: false },
            email: { nullable: true },
            ssn: { nullable: true },
            status: { nullable: false }
          }
        }
      ],
      {
        ExportMultiUser: [
          {
            id: 'e1',
            userId: 'user-1',
            email: 'alice@example.com',
            ssn: '111-11-1111',
            status: 'active'
          },
          {
            id: 'e2',
            userId: 'user-1',
            email: 'alice-alt@example.com',
            ssn: '111-11-2222',
            status: 'inactive'
          },
          {
            id: 'e3',
            userId: 'user-2',
            email: 'bob@example.com',
            ssn: '222-22-2222',
            status: 'active'
          },
          {
            id: 'e4',
            userId: 'user-3',
            email: 'charlie@example.com',
            ssn: '333-33-3333',
            status: 'active'
          }
        ]
      }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.export('user-1');

    expect(result.userId).toBe('user-1');
    expect(result.entities['ExportMultiUser']).toHaveLength(2);

    // Should export only user-1's records with id + PII/PHI fields (not 'status' which is 'none')
    expect(result.entities['ExportMultiUser']).toEqual([
      { id: 'e1', email: 'alice@example.com', ssn: '111-11-1111' },
      { id: 'e2', email: 'alice-alt@example.com', ssn: '111-11-2222' }
    ]);

    // Verify no data leakage from other users
    const allExportedRecords = result.entities['ExportMultiUser'];
    const hasUser2Data = allExportedRecords.some(
      (r: Record<string, unknown>) =>
        r.email === 'bob@example.com' || r.ssn === '222-22-2222'
    );
    const hasUser3Data = allExportedRecords.some(
      (r: Record<string, unknown>) =>
        r.email === 'charlie@example.com' || r.ssn === '333-33-3333'
    );

    expect(hasUser2Data).toBe(false);
    expect(hasUser3Data).toBe(false);
  });

  it('returns empty entities when no records match the user', async () => {
    registerEntity(
      'ExportNoMatch',
      { id: 'none', ownerId: 'none', data: 'pii' },
      { userIdField: 'ownerId' }
    );

    const { orm } = makeOrm(
      [
        {
          className: 'ExportNoMatch',
          properties: {
            id: { nullable: false },
            ownerId: { nullable: false },
            data: { nullable: true }
          }
        }
      ],
      {
        ExportNoMatch: [
          { id: 'x1', ownerId: 'user-99', data: 'secret1' },
          { id: 'x2', ownerId: 'user-88', data: 'secret2' }
        ]
      }
    );

    const service = new ComplianceDataService(orm as never, otel);
    const result = await service.export('user-1');

    expect(result.userId).toBe('user-1');
    // Entity should not appear in result when no records match
    expect(result.entities['ExportNoMatch']).toBeUndefined();
  });
});
