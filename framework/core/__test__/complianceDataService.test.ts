import { describe, expect, it, beforeEach } from 'vitest';
import type { MikroORM, EntitySchema } from '@mikro-orm/core';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import { ComplianceDataService } from '../src/services/complianceDataService';

// Mock OpenTelemetry collector
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const mockOtel: any = {
  info: () => {},
  warn: () => {},
  error: () => {}
};

// Mock entity records
const mockUsers = [
  { id: 'user-1', email: 'alice@example.com', name: 'Alice', status: 'active' },
  { id: 'user-2', email: 'bob@example.com', name: 'Bob', status: 'inactive' }
];

const mockAccounts = [
  { id: 'acc-1', userId: 'user-1', balance: 1000, accountNumber: '1234-5678' },
  { id: 'acc-2', userId: 'user-2', balance: 2000, accountNumber: '8765-4321' }
];

// Define test entities
const UserEntity = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().compliance('pii'),
    name: fp.string().compliance('pii'),
    status: fp.string().compliance('none')
  }
});

const AccountEntity = defineComplianceEntity({
  name: 'Account',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    userId: fp.uuid().compliance('none'),
    balance: fp.integer().compliance('none'),
    accountNumber: fp.string().compliance('pci')
  }
});

const PublicEntity = defineComplianceEntity({
  name: 'Public',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    label: fp.string().compliance('none')
  }
});

// Helper to create custom mock ORM with specific entities and records
function createCustomMockOrm(
  entities: EntitySchema[],
  recordsByEntity: Record<string, Record<string, unknown>[]>,
  _onRemove?: (entity: Record<string, unknown>) => void
): MikroORM {
  const metadata = new Map();

  for (const schema of entities) {
    metadata.set(schema.meta.name ?? 'Unknown', schema.meta);
  }

  const removed: Record<string, unknown>[] = [];

  const em = {
    find: async (entityClass: unknown, where?: Record<string, unknown>) => {
      let entityName: string;
      if (typeof entityClass === 'string') {
        entityName = entityClass;
      } else if (
        entityClass &&
        (typeof entityClass === 'object' ||
          typeof entityClass === 'function') &&
        'meta' in entityClass
      ) {
        const withMeta = entityClass as { meta?: { name?: string } };
        entityName = withMeta.meta?.name ?? 'Unknown';
      } else {
        entityName = 'Unknown';
      }

      const records = recordsByEntity[entityName] ?? [];

      if (!where || Object.keys(where).length === 0) {
        return records;
      }

      const filtered = records.filter((record) => {
        return Object.entries(where).every(
          ([key, value]) => record[key] === value
        );
      });
      return filtered;
    },
    remove: (entity: unknown) => {
      const rec = entity as Record<string, unknown>;
      removed.push(rec);
    },
    flush: async () => {
      for (const entity of removed) {
        for (const records of Object.values(recordsByEntity)) {
          const idx = records.indexOf(entity);
          if (idx >= 0) {
            records.splice(idx, 1);
          }
        }
      }
      removed.length = 0;
    }
  };

  return {
    em: {
      fork: () => em
    },
    getMetadata: () => ({
      getAll: () => metadata
    })
  } as unknown as MikroORM;
}

// Mock ORM
function createMockOrm(entities: EntitySchema[]): MikroORM {
  const recordsByEntity: Record<string, Record<string, unknown>[]> = {
    User: [...mockUsers],
    Account: [...mockAccounts],
    Public: []
  };

  return createCustomMockOrm(entities, recordsByEntity);
}

describe('ComplianceDataService - Option 3 (Auto-Discovery)', () => {
  let orm: MikroORM;

  beforeEach(() => {
    // Reset mock data
    mockUsers.length = 0;
    mockUsers.push(
      {
        id: 'user-1',
        email: 'alice@example.com',
        name: 'Alice',
        status: 'active'
      },
      {
        id: 'user-2',
        email: 'bob@example.com',
        name: 'Bob',
        status: 'inactive'
      }
    );
    mockAccounts.length = 0;
    mockAccounts.push(
      {
        id: 'acc-1',
        userId: 'user-1',
        balance: 1000,
        accountNumber: '1234-5678'
      },
      {
        id: 'acc-2',
        userId: 'user-2',
        balance: 2000,
        accountNumber: '8765-4321'
      }
    );

    orm = createMockOrm([UserEntity, AccountEntity, PublicEntity]);
  });

  it('auto-discovers entities from ORM metadata by default', async () => {
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('User');
    expect(result.entitiesAffected).toContain('Account');
    // User is anonymized (default), Account is anonymized (default)
    expect(result.recordsAnonymized).toBe(2);
    expect(result.recordsDeleted).toBe(0);
  });

  it('auto-discovers when explicitly enabled', async () => {
    const service = new ComplianceDataService(orm, mockOtel, {
      autoDiscover: true
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('User');
    expect(result.entitiesAffected).toContain('Account');
    expect(result.recordsAnonymized).toBe(2);
    expect(result.recordsDeleted).toBe(0);
  });

  it('skips entities with no protected data', async () => {
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).not.toContain('Public');
  });

  it('exports data from all entities with protected fields', async () => {
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.export('user-1');

    expect(result.userId).toBe('user-1');
    expect(result.entities['User']).toBeDefined();
    expect(result.entities['User']).toHaveLength(1);
    expect(result.entities['User'][0]).toEqual({
      id: 'user-1',
      email: 'alice@example.com',
      name: 'Alice'
    });
    expect(result.entities['Account']).toBeDefined();
    expect(result.entities['Account'][0]).toEqual({
      id: 'acc-1',
      accountNumber: '1234-5678'
    });
  });
});

describe('ComplianceDataService - Option 1 (Explicit Entities)', () => {
  it('processes only explicitly provided entities', async () => {
    const orm = createMockOrm([UserEntity, AccountEntity, PublicEntity]);

    // Only provide UserEntity explicitly
    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [UserEntity]
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('User');
    expect(result.entitiesAffected).not.toContain('Account'); // Not in explicit list
    expect(result.recordsAnonymized).toBe(1); // Only user (anonymized by default)
  });

  it('allows empty explicit entity list', async () => {
    const orm = createMockOrm([UserEntity, AccountEntity]);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: []
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toEqual([]);
    expect(result.recordsDeleted).toBe(0);
  });

  it('exports only from explicit entities', async () => {
    const orm = createMockOrm([UserEntity, AccountEntity]);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [AccountEntity]
    });

    const result = await service.export('user-1');

    expect(result.entities['Account']).toBeDefined();
    expect(result.entities['User']).toBeUndefined(); // Not in explicit list
  });

  it('explicit entities take precedence over autoDiscover', async () => {
    const orm = createMockOrm([UserEntity, AccountEntity]);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [UserEntity],
      autoDiscover: true // Should be ignored when entities provided
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('User');
    expect(result.entitiesAffected).not.toContain('Account');
  });
});

describe('ComplianceDataService - SOX compliance', () => {
  it('processes sox-classified fields', async () => {
    const SoxEntity = defineComplianceEntity({
      name: 'FinancialRecord',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        userId: fp.uuid().compliance('none'),
        auditLog: fp.string().compliance('sox')
      }
    });

    const mockRecords = [
      { id: 'fin-1', userId: 'user-1', auditLog: 'Financial audit log entry' }
    ];

    const recordsByEntity: Record<string, Record<string, unknown>[]> = {
      FinancialRecord: [...mockRecords]
    };

    const orm = createCustomMockOrm([SoxEntity], recordsByEntity);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [SoxEntity]
    });

    const result = await service.export('user-1');

    // SOX data should be exported (sox is protected data)
    expect(result.entities['FinancialRecord']).toBeDefined();
    expect(result.entities['FinancialRecord']).toHaveLength(1);
    expect(result.entities['FinancialRecord'][0]).toEqual({
      id: 'fin-1',
      auditLog: 'Financial audit log entry'
    });
  });
});

describe('ComplianceDataService - Export comprehensive', () => {
  it('exports only protected fields, not all fields', async () => {
    const orm = createMockOrm([UserEntity]);
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.export('user-1');

    // Should include protected fields
    expect(result.entities['User'][0]).toHaveProperty('email');
    expect(result.entities['User'][0]).toHaveProperty('name');
    // Should NOT include non-protected fields (status is 'none')
    expect(result.entities['User'][0]).not.toHaveProperty('status');
  });

  it('exports from multiple users returns only requested user data', async () => {
    const orm = createMockOrm([UserEntity]);
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.export('user-2');

    expect(result.userId).toBe('user-2');
    expect(result.entities['User']).toHaveLength(1);

    const userData = result.entities['User'][0] as Record<string, unknown>;
    expect(userData).toEqual({
      id: 'user-2',
      email: 'bob@example.com',
      name: 'Bob'
    });
    // Should NOT include user-1 data
    expect(userData.email).not.toBe('alice@example.com');
  });

  it('exports empty object when user has no data', async () => {
    const orm = createMockOrm([UserEntity, AccountEntity]);
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.export('user-999');

    expect(result.userId).toBe('user-999');
    expect(result.entities).toEqual({});
  });

  it('exports handles missing userIdField gracefully', async () => {
    const NoLinkEntity = defineComplianceEntity({
      name: 'NoUserLink',
      properties: {
        pk: fp.uuid().primary().compliance('none'),
        data: fp.string().compliance('pii')
      }
    });

    const orm = createMockOrm([NoLinkEntity]);
    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [NoLinkEntity]
    });

    const result = await service.export('user-1');

    // Should skip entity without user link
    expect(result.entities['NoUserLink']).toBeUndefined();
  });
});

describe('ComplianceDataService - Erase comprehensive', () => {
  it('returns zero counts when no data found', async () => {
    const orm = createMockOrm([UserEntity, AccountEntity]);
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.erase('user-999');

    expect(result.entitiesAffected).toEqual([]);
    expect(result.recordsDeleted).toBe(0);
    expect(result.recordsAnonymized).toBe(0);
  });

  it('anonymizes by default (no retention policy)', async () => {
    const orm = createMockOrm([UserEntity]);
    const service = new ComplianceDataService(orm, mockOtel);

    const result = await service.erase('user-1');

    expect(result.recordsAnonymized).toBe(1);
    expect(result.recordsDeleted).toBe(0);

    // Verify PII fields were nulled
    expect(mockUsers[0].email).toBeNull();
    expect(mockUsers[0].name).toBeNull();
    // Non-PII fields unchanged
    expect(mockUsers[0].id).toBe('user-1');
    expect(mockUsers[0].status).toBe('active');
  });

  it('deletes when retention policy action is delete', async () => {
    const DeleteEntity = defineComplianceEntity({
      name: 'Session',
      retention: {
        duration: 'P30D',
        action: 'delete' // Explicit delete
      },
      properties: {
        id: fp.uuid().primary().compliance('none'),
        userId: fp.uuid().compliance('none'),
        token: fp.string().compliance('pii'),
        createdAt: fp.datetime().compliance('none') // Required for retention policy
      }
    });

    const mockSessions = [{ id: 'sess-1', userId: 'user-1', token: 'abc123' }];

    const recordsByEntity: Record<string, Record<string, unknown>[]> = {
      Session: [...mockSessions]
    };

    const orm = createCustomMockOrm([DeleteEntity], recordsByEntity);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [DeleteEntity]
    });

    const result = await service.erase('user-1');

    expect(result.recordsDeleted).toBe(1);
    expect(result.recordsAnonymized).toBe(0);
    expect(recordsByEntity['Session']).toHaveLength(0); // Record removed
  });

  it('sets complianceErasedAt timestamp when anonymizing', async () => {
    const UserWithTimestamp = defineComplianceEntity({
      name: 'UserWithTimestamp',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        email: fp.string().compliance('pii'),
        complianceErasedAt: fp.datetime().nullable().compliance('none')
      }
    });

    const mockData = [
      {
        id: 'user-1',
        userId: 'user-1',
        email: 'test@example.com',
        complianceErasedAt: null
      }
    ];

    const recordsByEntity: Record<string, Record<string, unknown>[]> = {
      UserWithTimestamp: [...mockData]
    };

    const orm = createCustomMockOrm([UserWithTimestamp], recordsByEntity);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [UserWithTimestamp]
    });

    const result = await service.erase('user-1');

    expect(result.recordsAnonymized).toBe(1);
    expect(mockData[0].email).toBeNull(); // PII nulled
    expect(mockData[0].complianceErasedAt).toBeInstanceOf(Date); // Timestamp set
  });

  it('processes all compliance levels (pii, phi, pci, sox)', async () => {
    const AllTypesEntity = defineComplianceEntity({
      name: 'AllTypes',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        userId: fp.uuid().compliance('none'),
        email: fp.string().compliance('pii'),
        ssn: fp.string().compliance('phi'),
        cardNumber: fp.string().compliance('pci'),
        auditLog: fp.string().compliance('sox')
      }
    });

    const mockRecords = [
      {
        id: 'rec-1',
        userId: 'user-1',
        email: 'test@example.com',
        ssn: '123-45-6789',
        cardNumber: '1234-5678-9012-3456',
        auditLog: 'audit entry'
      }
    ];

    const recordsByEntity: Record<string, Record<string, unknown>[]> = {
      AllTypes: [...mockRecords]
    };

    const orm = createCustomMockOrm([AllTypesEntity], recordsByEntity);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [AllTypesEntity]
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('AllTypes');
    expect(result.recordsAnonymized).toBe(1); // Default action is anonymize
    // Verify PII fields were nulled
    expect(mockRecords[0].email).toBeNull();
    expect(mockRecords[0].ssn).toBeNull();
    expect(mockRecords[0].cardNumber).toBeNull();
    expect(mockRecords[0].auditLog).toBeNull();
    // ID fields remain
    expect(mockRecords[0].id).toBe('rec-1');
    expect(mockRecords[0].userId).toBe('user-1');
  });

  it('handles entities with custom userIdField from metadata', async () => {
    const CustomFieldEntity = defineComplianceEntity({
      name: 'CustomField',
      userIdField: 'ownerId', // Custom field name
      properties: {
        id: fp.uuid().primary().compliance('none'),
        ownerId: fp.uuid().compliance('none'),
        secret: fp.string().compliance('pii')
      }
    });

    const mockRecords = [
      { id: 'rec-1', ownerId: 'user-1', secret: 'confidential' }
    ];

    const recordsByEntity: Record<string, Record<string, unknown>[]> = {
      CustomField: [...mockRecords]
    };

    const orm = createCustomMockOrm([CustomFieldEntity], recordsByEntity);

    const service = new ComplianceDataService(orm, mockOtel, {
      entities: [CustomFieldEntity]
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('CustomField');
    expect(result.recordsAnonymized).toBe(1);
    // Verify PII was nulled
    expect(mockRecords[0].secret).toBeNull();
  });
});

describe('ComplianceDataService - Legacy constructor', () => {
  it('supports legacy userIdFieldOverrides parameter', async () => {
    const CustomEntity = defineComplianceEntity({
      name: 'Subscription',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        partyId: fp.uuid().compliance('none'),
        plan: fp.string().compliance('pii')
      }
    });

    const mockSubs = [{ id: 'sub-1', partyId: 'user-1', plan: 'premium' }];

    const recordsByEntity: Record<string, Record<string, unknown>[]> = {
      Subscription: [...mockSubs]
    };

    const orm = createCustomMockOrm([CustomEntity], recordsByEntity);

    // Legacy constructor: pass userIdFieldOverrides directly
    const service = new ComplianceDataService(orm, mockOtel, {
      Subscription: 'partyId'
    });

    const result = await service.erase('user-1');

    expect(result.entitiesAffected).toContain('Subscription');
    expect(result.recordsAnonymized).toBe(1); // Default is anonymize
  });
});
