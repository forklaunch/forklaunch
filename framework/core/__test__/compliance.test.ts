import { describe, expect, it } from 'vitest';
import { Platform } from '@mikro-orm/core';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import {
  getComplianceMetadata,
  getEntityComplianceFields,
  entityHasEncryptedFields,
  COMPLIANCE_KEY
} from '../src/persistence/complianceTypes';
import { EncryptedType } from '../src/persistence/encryptedType';

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

describe('fp encrypted type resolution', () => {
  function getEncryptedType(builder: unknown): EncryptedType | undefined {
    const opts = (builder as Record<string | symbol, unknown>)['~options'] as
      | Record<string, unknown>
      | undefined;
    const type = opts?.type;
    return type instanceof EncryptedType ? type : undefined;
  }

  it('resolves datetime to Date runtimeType', () => {
    const builder = fp.datetime().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et).toBeInstanceOf(EncryptedType);
    expect(et!.runtimeType).toBe('Date');
  });

  it('resolves integer to number runtimeType', () => {
    const builder = fp.integer().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('number');
  });

  it('resolves double to number runtimeType', () => {
    const builder = fp.double().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('number');
  });

  it('resolves float to number runtimeType', () => {
    const builder = fp.float().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('number');
  });

  it('resolves smallint to number runtimeType', () => {
    const builder = fp.smallint().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('number');
  });

  it('resolves mediumint to number runtimeType', () => {
    const builder = fp.mediumint().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('number');
  });

  it('resolves tinyint to number runtimeType', () => {
    const builder = fp.tinyint().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('number');
  });

  it('resolves bigint to bigint runtimeType', () => {
    const builder = fp.bigint().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('bigint');
  });

  it('resolves boolean to boolean runtimeType', () => {
    const builder = fp.boolean().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('boolean');
  });

  it('resolves string to string runtimeType', () => {
    const builder = fp.string().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('string');
  });

  it('resolves uuid to string runtimeType', () => {
    const builder = fp.uuid().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('string');
  });

  it('resolves json to any runtimeType (object)', () => {
    const builder = fp.json().compliance('pii');
    const et = getEncryptedType(builder);
    // json runtimeType is 'any', EncryptedType returns 'any'
    expect(et!.runtimeType).toBe('any');
  });

  it('resolves decimal to string runtimeType', () => {
    const builder = fp.decimal().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('string');
  });

  it('resolves blob to Buffer runtimeType', () => {
    const builder = fp.blob().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('Buffer');
  });

  it('resolves uint8array to Buffer runtimeType', () => {
    const builder = fp.uint8array().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('Buffer');
  });

  it('resolves p.array() as array container', () => {
    const builder = fp.array().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et).toBeInstanceOf(EncryptedType);
    // array container — runtimeType is 'object'
    expect(et!.runtimeType).toBe('object');
  });

  it('resolves integer().array() as array of numbers', () => {
    const builder = fp.integer().array().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et!.runtimeType).toBe('object');
    // Verify it actually hydrates numbers via a roundtrip
    const platform = {} as Platform;
    const encrypted = et!.convertToDatabaseValue([1, 2, 3], platform);
    const decrypted = et!.convertToJSValue(encrypted, platform);
    expect(decrypted).toEqual([1, 2, 3]);
  });

  it('resolves datetime().array() as array of Dates', () => {
    const builder = fp.datetime().array().compliance('pii');
    const et = getEncryptedType(builder);
    const platform = {} as Platform;
    const dates = [new Date('2024-01-01'), new Date('2024-06-15')];
    const encrypted = et!.convertToDatabaseValue(dates, platform);
    const decrypted = et!.convertToJSValue(encrypted, platform) as Date[];
    expect(decrypted).toHaveLength(2);
    expect(decrypted[0]).toBeInstanceOf(Date);
    expect(decrypted[0].toISOString()).toBe(dates[0].toISOString());
  });

  it('resolves enum().array() as array of strings', () => {
    const builder = fp
      .enum(['a', 'b', 'c'] as const)
      .array()
      .compliance('pii');
    const et = getEncryptedType(builder);
    const platform = {} as Platform;
    const encrypted = et!.convertToDatabaseValue(['a', 'b'], platform);
    const decrypted = et!.convertToJSValue(encrypted, platform);
    expect(decrypted).toEqual(['a', 'b']);
  });

  it('does not apply EncryptedType for none compliance', () => {
    const builder = fp.string().compliance('none');
    const et = getEncryptedType(builder);
    expect(et).toBeUndefined();
  });

  it('resolves lazy enum factory with encrypted compliance', () => {
    const Gender = { Male: 'male', Female: 'female', Other: 'other' } as const;
    const builder = fp
      .enum(() => Gender)
      .nullable()
      .compliance('pii');
    const et = getEncryptedType(builder);
    expect(et).toBeInstanceOf(EncryptedType);

    // Validate that allowed values were extracted from the lazy factory
    const platform = {} as Platform;
    const encrypted = et!.convertToDatabaseValue('female', platform);
    expect(typeof encrypted).toBe('string');
    const decrypted = et!.convertToJSValue(encrypted, platform);
    expect(decrypted).toBe('female');
  });

  it('rejects invalid values for lazy enum factory with encrypted compliance', () => {
    const Gender = { Male: 'male', Female: 'female', Other: 'other' } as const;
    const builder = fp.enum(() => Gender).compliance('pii');
    const et = getEncryptedType(builder);
    const platform = {} as Platform;
    expect(() => et!.convertToDatabaseValue('invalid', platform)).toThrow(
      /Invalid enum value: invalid/
    );
  });

  it('strips enum metadata for encrypted compliance (no DB check constraint)', () => {
    const Status = { Active: 'active', Inactive: 'inactive' } as const;
    const builder = fp.enum(() => Status).compliance('pii');
    const opts = (builder as Record<string | symbol, unknown>)['~options'] as
      | Record<string, unknown>
      | undefined;
    // enum metadata should be removed so MikroORM won't generate check constraints
    expect(opts?.items).toBeUndefined();
    expect(opts?.enum).toBeUndefined();
    expect(opts?.nativeEnumName).toBeUndefined();
    // column type should be text (encrypted ciphertext)
    expect(opts?.columnType).toBe('text');
  });

  it('resolves enum with direct items and encrypted compliance', () => {
    const builder = fp
      .enum(['red', 'green', 'blue'] as const)
      .compliance('pii');
    const et = getEncryptedType(builder);
    const platform = {} as Platform;

    // Valid value works
    const encrypted = et!.convertToDatabaseValue('red', platform);
    const decrypted = et!.convertToJSValue(encrypted, platform);
    expect(decrypted).toBe('red');

    // Invalid value throws
    expect(() => et!.convertToDatabaseValue('purple', platform)).toThrow(
      /Invalid enum value: purple/
    );
  });

  it('resolves bare enum() with encrypted compliance (no items)', () => {
    const builder = fp.enum().compliance('pii');
    const et = getEncryptedType(builder);
    expect(et).toBeInstanceOf(EncryptedType);
    // No items → no enum validation, but encryption still works
    const platform = {} as Platform;
    const encrypted = et!.convertToDatabaseValue('anything', platform);
    const decrypted = et!.convertToJSValue(encrypted, platform);
    expect(decrypted).toBe('anything');
  });
});
