import { describe, expect, it } from 'vitest';
import { createConfigInjector } from '../src/services/configInjector';
import { Lifetime } from '../src/services/types/configInjector.types';
import { SchemaValidator } from '@forklaunch/validator/typebox';

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
        type: Object,
        factory: (_args, context?: { tenantId?: string }) => ({
          tenantId: context?.tenantId
        })
      },
      Consumer: {
        lifetime: Lifetime.Scoped,
        type: Object,
        factory: (_args, _context, resolve) => ({
          first: resolve!('Bound', { tenantId: 'org' }) as { tenantId?: string },
          second: resolve!('Bound', { tenantId: '' }) as { tenantId?: string }
        })
      }
    });
    const consumer = ci.scopedResolver('Consumer')() as {
      first: { tenantId?: string };
      second: { tenantId?: string };
    };
    expect(consumer.first.tenantId).toBe('org');
    // The second request asked for '' and did not get it.
    expect(consumer.second).toBe(consumer.first);
    expect(consumer.second.tenantId).toBe('org');
  });
});
