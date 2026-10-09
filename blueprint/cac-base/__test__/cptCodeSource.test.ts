/**
 * Compliance entities (CptCode, imported transitively) require a
 * registered field encryptor before any entity module is imported — same
 * requirement as codeSetProviderResolver.test.ts.
 */
import { FieldEncryptor, registerEncryptor } from '@forklaunch/core/persistence';
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

import { EntityManager } from '@mikro-orm/postgresql';
import { CptCode } from '../persistence/entities/cptCode.entity';
import { EntityManagerCptCodeSource } from '../services/cptCodeSource.service';

// This is the concrete CptCodeSource the README (implementations/cac/base)
// holds up as the ready-to-use starting point for a real CPT connector —
// unlike CptCodeProvider itself (tested against a fake source), nothing
// previously verified this class actually queries CptCode correctly.
function fakeEm(findOneResult: unknown): EntityManager {
  return {
    findOne: vi.fn(async () => findOneResult)
  } as unknown as EntityManager;
}

describe('EntityManagerCptCodeSource', () => {
  it('looks up a code scoped to the given organization', async () => {
    const em = fakeEm({ code: '10001', description: 'Synthetic code' });
    const source = new EntityManagerCptCodeSource(em, 'org-1');

    const result = await source.lookup('10001');

    expect(result).toEqual({ code: '10001', description: 'Synthetic code' });
    expect(em.findOne).toHaveBeenCalledWith(CptCode, {
      code: '10001',
      organizationId: 'org-1'
    });
  });

  it('returns undefined when no row matches for that organization', async () => {
    const em = fakeEm(null);
    const source = new EntityManagerCptCodeSource(em, 'org-1');

    const result = await source.lookup('99999');

    expect(result).toBeUndefined();
  });

  it('scopes the lookup to a different organization independently', async () => {
    const em = fakeEm(null);
    const source = new EntityManagerCptCodeSource(em, 'org-2');

    await source.lookup('10001');

    expect(em.findOne).toHaveBeenCalledWith(CptCode, {
      code: '10001',
      organizationId: 'org-2'
    });
  });
});
