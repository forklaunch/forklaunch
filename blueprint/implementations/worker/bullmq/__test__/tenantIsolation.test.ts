import { QueueOptions } from 'bullmq';
import { describe, expect, it, vi } from 'vitest';
import {
  bullPrefixFor,
  conflictsWithTenantPrefix,
  isTenantIsolationError,
  makeIsolationReporter,
  tenantKeyPrefix,
  withTenantPrefix
} from '../domain/tenantIsolation';

const partition = { REDIS_KEY_PREFIX: 'fl_production_fit_momma_ada24134:' };

describe('tenant key prefix', () => {
  it('reads the platform-injected prefix, preferring the plain name', () => {
    expect(tenantKeyPrefix(partition)).toBe(
      'fl_production_fit_momma_ada24134:'
    );
    expect(tenantKeyPrefix({ FORKLAUNCH_REDIS_KEY_PREFIX: 'fl_x:' })).toBe(
      'fl_x:'
    );
  });

  it('is undefined on a dedicated cache, where BullMQ default is right', () => {
    expect(tenantKeyPrefix({})).toBeUndefined();
    expect(tenantKeyPrefix({ REDIS_KEY_PREFIX: '  ' })).toBeUndefined();
  });

  it('drops the trailing separator, because BullMQ adds its own', () => {
    // `fl_x:` + BullMQ's `:` would key everything under `fl_x::<queue>` —
    // inside the ACL pattern, but not where anything else looks.
    expect(bullPrefixFor(partition)).toBe('fl_production_fit_momma_ada24134');
    expect(bullPrefixFor({ REDIS_KEY_PREFIX: 'fl_x' })).toBe('fl_x');
  });
});

describe('withTenantPrefix', () => {
  it('forces the prefix rather than defaulting it', () => {
    const forced = withTenantPrefix(
      { prefix: 'bull' } as unknown as QueueOptions,
      partition
    );
    expect(forced.prefix).toBe('fl_production_fit_momma_ada24134');
  });

  it('reports a caller prefix that would have been overridden', () => {
    expect(
      conflictsWithTenantPrefix(
        { prefix: 'bull' } as unknown as QueueOptions,
        partition
      )
    ).toBe(true);
    expect(
      conflictsWithTenantPrefix(
        {
          prefix: 'fl_production_fit_momma_ada24134'
        } as unknown as QueueOptions,
        partition
      )
    ).toBe(false);
  });

  it('leaves options untouched with no partition, so dedicated is unaffected', () => {
    const opts = { connection: {} } as unknown as QueueOptions;
    expect(withTenantPrefix(opts, {})).toBe(opts);
  });
});

describe('isTenantIsolationError', () => {
  it('recognises both shapes the server actually sent', () => {
    // Verbatim from production, 2026-09-28.
    expect(
      isTenantIsolationError(
        new Error(
          "NOPERM User fl_production_fit_momma_ada24134 has no permissions to run the 'info' command"
        )
      )
    ).toBe(true);
    expect(
      isTenantIsolationError(new Error('NOPERM No permissions to access a key'))
    ).toBe(true);
  });

  it('does not swallow ordinary faults that SHOULD be retried', () => {
    expect(isTenantIsolationError(new Error('ECONNRESET'))).toBe(false);
    expect(isTenantIsolationError(new Error('READONLY replica'))).toBe(false);
    expect(isTenantIsolationError(undefined)).toBe(false);
  });
});

describe('makeIsolationReporter', () => {
  it('speaks once per process, however many times it is told', () => {
    const log = vi.fn();
    const report = makeIsolationReporter(log);
    for (let i = 0; i < 10_000; i++) {
      report('notify', new Error('NOPERM No permissions to access a key'));
    }
    expect(log).toHaveBeenCalledTimes(1);
    expect(log.mock.calls[0][0]).toContain('configuration failure');
  });
});
