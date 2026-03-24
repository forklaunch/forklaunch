import { describe, expect, it } from 'vitest';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import {
  getComplianceMetadata,
  getEntityComplianceFields,
  entityHasEncryptedFields,
  COMPLIANCE_KEY
} from '../src/persistence/complianceTypes';

/** Read a property from an object by key, avoiding type casts. */
function readKey(obj: unknown, key: string): unknown {
  if (obj == null || typeof obj !== 'object') return undefined;
  return (obj as Record<string, unknown>)[key];
}

describe('fp property builder', () => {
  it('adds .compliance() to scalar builders', () => {
    const builder = fp.string();
    expect(typeof builder.compliance).toBe('function');
  });

  it('stores compliance level via .compliance()', () => {
    const classified = fp.string().compliance('pii');
    expect(readKey(classified, COMPLIANCE_KEY)).toBe('pii');
  });

  it('compliance works at end of chain', () => {
    const classified = fp.string().nullable().unique().compliance('phi');
    expect(readKey(classified, COMPLIANCE_KEY)).toBe('phi');
  });

  it('compliance works with different chain order', () => {
    const classified = fp.string().unique().nullable().compliance('pci');
    expect(readKey(classified, COMPLIANCE_KEY)).toBe('pci');
  });

  it('auto-classifies relation methods as none', () => {
    const relation = fp.manyToOne(() => ({}));
    expect(readKey(relation, COMPLIANCE_KEY)).toBe('none');
  });

  it('preserves auto-classification through relation chaining', () => {
    const relation = fp.manyToOne(() => ({})).nullable();
    expect(readKey(relation, COMPLIANCE_KEY)).toBe('none');
  });

  it('forwards ~options for MikroORM compatibility', () => {
    const builder = fp.string().compliance('none');
    expect(readKey(builder, '~options')).toBeDefined();
  });
});

describe('defineComplianceEntity', () => {
  it('registers compliance metadata for all fields', () => {
    defineComplianceEntity({
      name: 'TestUser',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        email: fp.string().compliance('pii'),
        ssn: fp.string().compliance('phi'),
        cardNum: fp.string().compliance('pci')
      }
    });

    expect(getComplianceMetadata('TestUser', 'id')).toBe('none');
    expect(getComplianceMetadata('TestUser', 'email')).toBe('pii');
    expect(getComplianceMetadata('TestUser', 'ssn')).toBe('phi');
    expect(getComplianceMetadata('TestUser', 'cardNum')).toBe('pci');
  });

  it('returns none for unregistered entities/fields', () => {
    expect(getComplianceMetadata('NonExistent', 'field')).toBe('none');
  });

  it('getEntityComplianceFields returns full map', () => {
    defineComplianceEntity({
      name: 'TestOrg',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        taxId: fp.string().compliance('pci')
      }
    });

    const fields = getEntityComplianceFields('TestOrg');
    expect(fields).toBeDefined();
    expect(fields!.get('id')).toBe('none');
    expect(fields!.get('taxId')).toBe('pci');
  });

  it('entityHasEncryptedFields detects phi/pci', () => {
    defineComplianceEntity({
      name: 'EncryptedEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        secret: fp.string().compliance('phi')
      }
    });
    expect(entityHasEncryptedFields('EncryptedEntity')).toBe(true);
  });

  it('entityHasEncryptedFields returns false for pii-only', () => {
    defineComplianceEntity({
      name: 'PiiOnlyEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        email: fp.string().compliance('pii')
      }
    });
    expect(entityHasEncryptedFields('PiiOnlyEntity')).toBe(false);
  });

  it('throws on missing compliance at runtime', () => {
    expect(() => {
      defineComplianceEntity({
        name: 'BadEntity',
        properties: {
          // @ts-expect-error — intentionally omitting .compliance() to test runtime validation
          email: fp.string()
        }
      });
    }).toThrow(/missing compliance classification/);
  });

  it('registers relations as none', () => {
    const Parent = defineComplianceEntity({
      name: 'Parent',
      properties: {
        id: fp.uuid().primary().compliance('none')
      }
    });

    defineComplianceEntity({
      name: 'Child',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        parent: () => fp.manyToOne(() => Parent).nullable()
      }
    });

    expect(getComplianceMetadata('Child', 'parent')).toBe('none');
  });

  it('returns a valid MikroORM EntitySchema', () => {
    const schema = defineComplianceEntity({
      name: 'ValidEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        name: fp.string().compliance('none')
      }
    });

    expect(schema.meta.className).toBe('ValidEntity');
  });
});
