import { describe, expect, it } from 'vitest';
import {
  FieldEncryptor,
  encryptionKeyId
} from '../src/persistence/fieldEncryptor';
import {
  classifyEncryptedValue,
  defaultTenantIdsFor,
  isEncryptedValue,
  rotationTotals,
  type RotationTableReport
} from '../src/persistence/keyRotation';

const CURRENT = 'current-master-key-2026-09';
const PREVIOUS = 'previous-master-key-2026-03';
const org = '302fbb63-1710-4738-aa60-d1fcabd2988f';
const ring = new FieldEncryptor(CURRENT, { previousKeys: [PREVIOUS] });
const current = new FieldEncryptor(CURRENT);
const previous = new FieldEncryptor(PREVIOUS);

describe('classifyEncryptedValue', () => {
  it('recognises the ciphertext envelope', () => {
    expect(isEncryptedValue(current.encrypt('x', ''))).toBe(true);
    expect(isEncryptedValue('enterprise-manual')).toBe(false);
    expect(isEncryptedValue(null)).toBe(false);
    expect(isEncryptedValue(42)).toBe(false);
  });

  it('leaves plaintext alone', () => {
    expect(classifyEncryptedValue('internal', ring, [''])).toEqual({
      kind: 'plaintext'
    });
    expect(classifyEncryptedValue(null, ring, [''])).toEqual({
      kind: 'plaintext'
    });
  });

  it('leaves values already under the current key alone', () => {
    expect(
      classifyEncryptedValue(current.encrypt('a', ''), ring, [''])
    ).toEqual({
      kind: 'current',
      tenantId: '',
      keyId: encryptionKeyId(CURRENT)
    });
  });

  it('rewrites a previous-key value under the current key, same tenant', () => {
    const outcome = classifyEncryptedValue(previous.encrypt('b', org), ring, [
      org,
      ''
    ]);
    expect(outcome.kind).toBe('rewritten');
    if (outcome.kind !== 'rewritten') return;
    expect(outcome.tenantId).toBe(org);
    expect(outcome.keyId).toBe(encryptionKeyId(PREVIOUS));
    // Deterministic IVs: the rewrite equals a fresh encryption, so WHERE lookups match.
    expect(outcome.next).toBe(current.encrypt('b', org));
    expect(() => current.decrypt(outcome.next, '')).toThrow();
  });

  it('falls back to every known tenant when the row hints are wrong', () => {
    const value = previous.encrypt('leaked', org);
    expect(classifyEncryptedValue(value, ring, [''])).toEqual({
      kind: 'unreadable'
    });
    const outcome = classifyEncryptedValue(value, ring, [''], ['other', org]);
    expect(outcome.kind).toBe('rewritten');
    if (outcome.kind !== 'rewritten') return;
    expect(outcome.tenantId).toBe(org);
  });

  it('reports what opens with no key', () => {
    const stranger = new FieldEncryptor('a-key-nobody-kept');
    expect(
      classifyEncryptedValue(
        stranger.encrypt('gone', ''),
        ring,
        ['', org],
        [org]
      )
    ).toEqual({ kind: 'unreadable' });
  });

  it('is a no-op the second time', () => {
    const first = classifyEncryptedValue(previous.encrypt('twice', org), ring, [
      org
    ]);
    if (first.kind !== 'rewritten') throw new Error('expected a rewrite');
    expect(classifyEncryptedValue(first.next, ring, [org]).kind).toBe(
      'current'
    );
  });
});

describe('tenant candidates and totals', () => {
  it('tries the organization id before the empty tenant', () => {
    expect(defaultTenantIdsFor('X', { organization_id: org })).toEqual([
      org,
      ''
    ]);
    expect(defaultTenantIdsFor('X', { organizationId: org })).toEqual([
      org,
      ''
    ]);
    expect(defaultTenantIdsFor('X', {})).toEqual(['']);
  });

  it('sums reports across tables', () => {
    const report = (n: number): RotationTableReport => ({
      table: `t${n}`,
      columns: ['c'],
      scanned: n,
      plaintext: 0,
      current: n - 1,
      rewritten: 1,
      unreadable: 0,
      unreadableIds: [],
      byKeyId: {},
      missingKeyIds: {}
    });
    expect(rotationTotals([report(3), report(5)])).toEqual({
      tables: 2,
      scanned: 8,
      rewritten: 2,
      current: 6,
      plaintext: 0,
      unreadable: 0
    });
  });
});

describe('format upgrade to v3', () => {
  const v3 = new FieldEncryptor(CURRENT, {
    previousKeys: [PREVIOUS],
    format: 'v3'
  });

  it('rewrites unstamped current-key values when the encryptor writes v3', () => {
    const outcome = classifyEncryptedValue(current.encrypt('a', org), v3, [
      org
    ]);
    expect(outcome.kind).toBe('rewritten');
    if (outcome.kind !== 'rewritten') return;
    expect(outcome.keyId).toBe(encryptionKeyId(CURRENT));
    expect(outcome.next).toBe(v3.encrypt('a', org));
    expect(classifyEncryptedValue(outcome.next, v3, [org]).kind).toBe(
      'current'
    );
  });

  it('leaves stamped current-key values alone under a v2 writer', () => {
    expect(classifyEncryptedValue(v3.encrypt('a', org), ring, [org]).kind).toBe(
      'current'
    );
  });
});
