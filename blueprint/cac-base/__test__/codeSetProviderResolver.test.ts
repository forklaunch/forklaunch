/**
 * Compliance entities (CodeSetLicense, imported transitively by the
 * resolver under test) require a registered field encryptor before any
 * entity module is imported — same requirement as
 * implementations/cac/base/__test__/test-utils.ts.
 */
import { FieldEncryptor, registerEncryptor } from '@forklaunch/core/persistence';
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { EntityManager } from '@mikro-orm/postgresql';
import { CodeSetType } from '../domain/enum/codeSetType.enum';
import { LicenseStatus } from '../domain/enum/licenseStatus.enum';
import { CodeSetProviderResolver } from '../services/codeSetProviderResolver.service';

const openTelemetryCollector = new OpenTelemetryCollector('test', 'info', {});

// Minimal fake EntityManager — the resolver only ever calls findOne(), so
// that's all a test double needs to implement. No real DB/testcontainers
// required, same reasoning as cptCodeProvider.test.ts's FakeCptCodeSource.
function fakeEm(findOneResult: unknown, shouldThrow = false): EntityManager {
  return {
    findOne: async () => {
      if (shouldThrow) throw new Error('db unavailable');
      return findOneResult;
    }
  } as unknown as EntityManager;
}

describe('CodeSetProviderResolver', () => {
  it('resolves the mock provider when no organizationId is given', async () => {
    const resolver = new CodeSetProviderResolver(
      fakeEm(null),
      openTelemetryCollector
    );

    const provider = await resolver.resolve(undefined);
    expect(provider.describe()).toEqual({
      codeSetType: 'mock',
      licensed: false
    });
  });

  it('resolves the mock provider when no active CPT license exists', async () => {
    const resolver = new CodeSetProviderResolver(
      fakeEm(null),
      openTelemetryCollector
    );

    const provider = await resolver.resolve('org-1');
    expect(provider.describe()).toEqual({
      codeSetType: 'mock',
      licensed: false
    });
  });

  it('resolves a real CPT provider when an active license exists', async () => {
    const resolver = new CodeSetProviderResolver(
      fakeEm({
        organizationId: 'org-1',
        codeSetType: CodeSetType.CPT,
        status: LicenseStatus.ACTIVE
      }),
      openTelemetryCollector
    );

    const provider = await resolver.resolve('org-1');
    expect(provider.describe()).toEqual({
      codeSetType: 'cpt',
      licensed: true
    });
  });

  // A license-lookup failure must propagate, not silently fall back to
  // mock: a transient DB blip for a *licensed* org would otherwise stamp
  // the claim codeSetType: 'mock' permanently (buildClaim never re-resolves
  // it, §5's "never retroactively recoded" rule), and every real CPT line
  // then reads as unrecognized to scrubClaim — falsely denying a clean
  // claim. A retryable 500 is safer than silently miscoding it.
  it('propagates a license lookup failure rather than falling back to mock', async () => {
    const resolver = new CodeSetProviderResolver(
      fakeEm(null, true),
      openTelemetryCollector
    );

    await expect(resolver.resolve('org-1')).rejects.toThrow('db unavailable');
  });
});
