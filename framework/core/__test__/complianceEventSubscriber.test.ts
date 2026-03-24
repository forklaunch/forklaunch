import { describe, expect, it, vi, beforeEach } from 'vitest';
import { FieldEncryptor } from '../src/encryption/fieldEncryptor';
import {
  ComplianceEventSubscriber,
  wrapEmWithNativeQueryBlocking
} from '../src/persistence/complianceEventSubscriber';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import type { EventArgs, EntityMetadata } from '@mikro-orm/core';

const MASTER_KEY = 'test-master-key-for-unit-tests-32ch';
const TENANT_ID = 'tenant-123';

// Register test entities
defineComplianceEntity({
  name: 'Patient',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    name: fp.string().compliance('pii'),
    ssn: fp.string().compliance('phi'),
    cardNumber: fp.string().compliance('pci'),
    status: fp.string().compliance('none')
  }
});

defineComplianceEntity({
  name: 'PublicEntity',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    label: fp.string().compliance('none')
  }
});

function makeEventArgs(
  entityName: string,
  entity: Record<string, unknown>,
  tenantId?: string
): EventArgs<unknown> {
  return {
    entity,
    meta: { className: entityName } as EntityMetadata<unknown>,
    em: {
      getFilterParams(filterName: string) {
        if (filterName === 'tenant') {
          return tenantId ? { tenantId } : undefined;
        }
        return undefined;
      }
    } as EventArgs<unknown>['em']
  };
}

function makeMockEm(overrides: Record<string, ReturnType<typeof vi.fn>> = {}) {
  return {
    nativeInsert: vi.fn(),
    nativeUpdate: vi.fn(),
    nativeDelete: vi.fn(),
    find: vi.fn().mockResolvedValue([]),
    ...overrides
  };
}

describe('ComplianceEventSubscriber', () => {
  let encryptor: FieldEncryptor;
  let subscriber: ComplianceEventSubscriber;

  beforeEach(() => {
    encryptor = new FieldEncryptor(MASTER_KEY);
    subscriber = new ComplianceEventSubscriber(encryptor);
  });

  describe('beforeCreate / beforeUpdate', () => {
    it('encrypts PHI fields before persist', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: '123-45-6789',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.ssn).toMatch(/^v1:/);
      expect(entity.ssn).not.toBe('123-45-6789');
    });

    it('encrypts PCI fields before persist', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: '123',
        cardNumber: '4111-1111-1111-1111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.cardNumber).toMatch(/^v1:/);
      expect(entity.cardNumber).not.toBe('4111-1111-1111-1111');
    });

    it('does NOT encrypt PII fields', async () => {
      const entity = {
        id: '1',
        name: 'John Doe',
        ssn: '123',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.name).toBe('John Doe');
    });

    it('does NOT encrypt none fields', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: '123',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.status).toBe('active');
      expect(entity.id).toBe('1');
    });

    it('skips null values', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: null,
        cardNumber: null,
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.ssn).toBeNull();
      expect(entity.cardNumber).toBeNull();
    });

    it('does not double-encrypt already encrypted values', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: 'v1:abc:def:ghi',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.ssn).toBe('v1:abc:def:ghi');
    });

    it('throws without tenant context', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: '123-45-6789',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity);

      await expect(subscriber.beforeCreate(args)).rejects.toThrow(
        /tenant context/i
      );
    });

    it('skips entities with no encrypted compliance fields', async () => {
      const entity = { id: '1', label: 'test' };
      const args = makeEventArgs('PublicEntity', entity, TENANT_ID);

      await subscriber.beforeCreate(args);

      expect(entity.label).toBe('test');
    });

    it('skips unregistered entities', async () => {
      const entity = { id: '1', data: 'foo' };
      const args = makeEventArgs('UnknownEntity', entity, TENANT_ID);

      await subscriber.beforeCreate(args);
      expect(entity.data).toBe('foo');
    });
  });

  describe('onLoad', () => {
    it('decrypts encrypted PHI fields on load', async () => {
      const encrypted = encryptor.encrypt('123-45-6789', TENANT_ID)!;
      const entity = {
        id: '1',
        name: 'John',
        ssn: encrypted,
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.onLoad(args);

      expect(entity.ssn).toBe('123-45-6789');
    });

    it('passes through pre-migration plaintext with warning', async () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const entity = {
        id: '1',
        name: 'John',
        ssn: 'plaintext-ssn',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await subscriber.onLoad(args);

      expect(entity.ssn).toBe('plaintext-ssn');
      expect(warnSpy).toHaveBeenCalledWith(
        expect.stringContaining('unencrypted phi data')
      );
      warnSpy.mockRestore();
    });

    it('throws on corrupted ciphertext', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: 'v1:bad:data:here',
        cardNumber: '4111',
        status: 'active'
      };
      const args = makeEventArgs('Patient', entity, TENANT_ID);

      await expect(subscriber.onLoad(args)).rejects.toThrow(/decrypt/i);
    });
  });

  describe('encrypt/decrypt roundtrip', () => {
    it('roundtrips PHI and PCI fields through create and load', async () => {
      const entity = {
        id: '1',
        name: 'John',
        ssn: '123-45-6789',
        cardNumber: '4111-1111',
        status: 'active'
      };

      await subscriber.beforeCreate(
        makeEventArgs('Patient', entity, TENANT_ID)
      );
      expect(entity.ssn).toMatch(/^v1:/);
      expect(entity.cardNumber).toMatch(/^v1:/);

      await subscriber.onLoad(makeEventArgs('Patient', entity, TENANT_ID));
      expect(entity.ssn).toBe('123-45-6789');
      expect(entity.cardNumber).toBe('4111-1111');
    });
  });
});

describe('wrapEmWithNativeQueryBlocking', () => {
  it('blocks nativeInsert on entities with phi/pci fields', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() => wrapped.nativeInsert('Patient' as never, {} as never)).toThrow(
      /nativeInsert.*blocked.*Patient/
    );
  });

  it('blocks nativeUpdate on entities with phi/pci fields', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() =>
      wrapped.nativeUpdate('Patient' as never, {} as never, {} as never)
    ).toThrow(/nativeUpdate.*blocked.*Patient/);
  });

  it('blocks nativeDelete on entities with phi/pci fields', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() => wrapped.nativeDelete('Patient' as never, {} as never)).toThrow(
      /nativeDelete.*blocked.*Patient/
    );
  });

  it('allows native queries on entities without phi/pci fields', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() =>
      wrapped.nativeInsert('PublicEntity' as never, {} as never)
    ).not.toThrow();
    expect(mockEm.nativeInsert).toHaveBeenCalled();
  });

  it('allows native queries on unregistered entities', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() =>
      wrapped.nativeInsert('UnregisteredEntity' as never, {} as never)
    ).not.toThrow();
  });

  it('passes through non-blocked methods', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    wrapped.find('Patient' as never, {} as never);
    expect(mockEm.find).toHaveBeenCalled();
  });
});
