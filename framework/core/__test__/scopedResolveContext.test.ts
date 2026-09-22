import { SchemaValidator } from '@forklaunch/validator/typebox';
import { describe, expect, it } from 'vitest';
import { createConfigInjector } from '../src/services/configInjector';
import { Lifetime } from '../src/services/types/configInjector.types';

class Bound {
  constructor(readonly tenantId: string | undefined) {}
}

class Consumer {
  constructor(
    readonly first: Bound,
    readonly second: Bound
  ) {}
}

/**
 * A factory that asks the scope for the same Scoped token twice, with two
 * different contexts, gets the same instance both times: `resolve` hands
 * back whatever the scope already holds and ignores the new context.
 *
 * This is what made billing's FeatureFlagService report the default plan for
 * every organization (forklaunch-platform#867): it resolved `EntityMgr` once
 * bound to the organization (for the subscription) and once bound to the
 * empty tenant (for the plan), got the organization-bound one back both
 * times, and the plan lookup ran under the wrong key.
 */
describe('scoped resolve with a context', () => {
  it('returns the cached scoped instance regardless of the context passed', () => {
    const ci = createConfigInjector(SchemaValidator(), {
      Bound: {
        lifetime: Lifetime.Scoped,
        type: Bound,
        factory: (_args, context) =>
          new Bound(
            typeof context.tenantId === 'string' ? context.tenantId : undefined
          )
      },
      Consumer: {
        lifetime: Lifetime.Scoped,
        type: Consumer,
        factory: (_args, _context, resolve) =>
          new Consumer(
            resolve!('Bound', { tenantId: 'org' }),
            resolve!('Bound', { tenantId: '' })
          )
      }
    });
    const consumer = ci.scopedResolver('Consumer')();
    expect(consumer.first.tenantId).toBe('org');
    // The second request asked for '' and did not get it.
    expect(consumer.second).toBe(consumer.first);
    expect(consumer.second.tenantId).toBe('org');
  });
});
