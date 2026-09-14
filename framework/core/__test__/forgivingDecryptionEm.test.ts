import { beforeEach, describe, expect, it, vi } from 'vitest';
import { registerEntityCompliance } from '../src/persistence/complianceTypes';
import { wrapEmWithForgivingDecryption } from '../src/persistence/forgivingDecryptionEm';

/**
 * The failure being softened: reading an entity whose encrypted columns were
 * written under a different tenant throws, and the endpoint 500s. Three
 * separate endpoints hit it in one week because the mistake is invisible until
 * a tenant id is actually in play.
 *
 * The retry re-issues the read with the encrypted columns excluded, rather than
 * nulling them during hydration. That distinction is not cosmetic: returning
 * `undefined` from the Type was measured against a real database to write NULL
 * over the ciphertext on the next flush, because MikroORM treats a hydrated
 * `undefined` as the loaded value. A `fields` selection is never hydrated at
 * all, so the column is excluded from the UPDATE.
 */

const DECRYPT_ERROR = new Error(
  'Failed to decrypt encrypted column value: Decryption failed: ciphertext is corrupted or the wrong key was used'
);

const logger = () => ({ warn: vi.fn() });

/** An EM that fails a full read and succeeds once given a `fields` selection. */
const buildEm = (opts: { failWithout?: boolean } = { failWithout: true }) => {
  const calls: { method: string; options?: Record<string, unknown> }[] = [];
  const make =
    (method: string) =>
    async (
      _entity: unknown,
      _where: unknown,
      options?: Record<string, unknown>
    ) => {
      calls.push({ method, options });
      if (opts.failWithout && !options?.fields) throw DECRYPT_ERROR;
      return { id: 'row-1', organization: 'org-1' };
    };

  return {
    calls,
    em: {
      findOne: vi.fn(make('findOne')),
      find: vi.fn(make('find')),
      findOneOrFail: vi.fn(make('findOneOrFail')),
      findAll: vi.fn(make('findAll')),
      findAndCount: vi.fn(make('findAndCount')),
      flush: vi.fn(async () => undefined),
      persist: vi.fn(),
      getMetadata: () => ({
        find: (_n: string) => ({
          props: {
            id: {},
            organization: {},
            memberEmail: {},
            memberName: {}
          }
        })
      })
    }
  };
};

beforeEach(() => {
  registerEntityCompliance(
    'Membership',
    new Map([
      ['id', 'none'],
      ['organization', 'none'],
      ['memberEmail', 'pii'],
      ['memberName', 'pii']
    ])
  );
});

describe('wrapEmWithForgivingDecryption', () => {
  it('retries without the encrypted columns instead of throwing', async () => {
    const { em, calls } = buildEm();
    const log = logger();
    const wrapped = wrapEmWithForgivingDecryption(em, log);

    const row = await wrapped.findOne('Membership', { id: 'row-1' });

    expect(row).toEqual({ id: 'row-1', organization: 'org-1' });
    expect(calls).toHaveLength(2);
    expect(calls[0].options?.fields).toBeUndefined();
    // The retry must select the complement, never the encrypted columns.
    expect(calls[1].options?.fields).toEqual(['id', 'organization']);
  });

  it('never selects an encrypted column on the retry', async () => {
    const { em, calls } = buildEm();
    const wrapped = wrapEmWithForgivingDecryption(em, logger());

    await wrapped.findOne('Membership', { id: 'row-1' });

    const fields = calls[1].options?.fields as string[];
    expect(fields).not.toContain('memberEmail');
    expect(fields).not.toContain('memberName');
  });

  it('logs a warning that names the entity, the dropped columns, and both fixes', async () => {
    const { em } = buildEm();
    const log = logger();
    const wrapped = wrapEmWithForgivingDecryption(em, log);

    await wrapped.findOne('Membership', { id: 'row-1' });

    expect(log.warn).toHaveBeenCalledTimes(1);
    const [message, meta] = log.warn.mock.calls[0];
    expect(message).toContain('Membership');
    expect(message).toContain('memberEmail');
    // Both remedies, so the reader does not have to guess which applies.
    expect(message).toContain('wrapEmWithTenantContext');
    expect(message).toContain('fields');
    // And the corruption case, which looks identical to a wrong tenant.
    expect(message).toContain('corruption');
    expect(meta).toMatchObject({
      entity: 'Membership',
      droppedColumns: ['memberEmail', 'memberName']
    });
  });

  it('says the row was not modified, since that is the first worry on seeing this', async () => {
    const { em } = buildEm();
    const log = logger();
    await wrapEmWithForgivingDecryption(em, log).findOne('Membership', {});
    expect(log.warn.mock.calls[0][0]).toContain('NOT modified');
  });

  it.each([
    'findOne',
    'findOneOrFail',
    'find',
    'findAll',
    'findAndCount'
  ] as const)('covers %s', async (method) => {
    const { em, calls } = buildEm();
    const wrapped = wrapEmWithForgivingDecryption(em, logger());

    await (
      wrapped as unknown as Record<
        string,
        (...a: unknown[]) => Promise<unknown>
      >
    )[method]('Membership', {});

    expect(calls).toHaveLength(2);
    expect(calls[1].options?.fields).toEqual(['id', 'organization']);
  });

  it('costs nothing when the read succeeds', async () => {
    const { em, calls } = buildEm({ failWithout: false });
    const log = logger();
    const wrapped = wrapEmWithForgivingDecryption(em, log);

    await wrapped.findOne('Membership', {});

    expect(calls).toHaveLength(1);
    expect(log.warn).not.toHaveBeenCalled();
  });

  it('rethrows errors that are not decryption failures', async () => {
    const em = {
      findOne: vi.fn(async (_entity: unknown, _where: unknown) => {
        throw new Error('connection terminated unexpectedly');
      }),
      getMetadata: () => ({ find: () => ({ props: {} }) })
    };
    const wrapped = wrapEmWithForgivingDecryption(em, logger());

    await expect(wrapped.findOne('Membership', {})).rejects.toThrow(
      'connection terminated'
    );
  });

  it('rethrows when the entity has no encrypted columns, since a retry would be identical', async () => {
    registerEntityCompliance('Plain', new Map([['label', 'none']]));
    const { em } = buildEm();
    const wrapped = wrapEmWithForgivingDecryption(em, logger());

    await expect(wrapped.findOne('Plain', {})).rejects.toThrow(
      'Failed to decrypt'
    );
  });

  it('passes non-read members straight through', async () => {
    const { em } = buildEm();
    const wrapped = wrapEmWithForgivingDecryption(em, logger());

    await wrapped.flush();
    expect(em.flush).toHaveBeenCalled();
  });

  it('preserves the caller’s other options on the retry', async () => {
    const { em, calls } = buildEm();
    const wrapped = wrapEmWithForgivingDecryption(em, logger());

    await wrapped.findOne('Membership', {}, { orderBy: { id: 'ASC' } });

    expect(calls[1].options).toMatchObject({
      orderBy: { id: 'ASC' },
      fields: ['id', 'organization']
    });
  });
});
