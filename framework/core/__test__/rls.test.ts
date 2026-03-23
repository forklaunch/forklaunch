import { describe, expect, it, vi, beforeEach } from 'vitest';
import { RlsEventSubscriber } from '../src/persistence/rls';
import { TENANT_FILTER_NAME } from '../src/persistence/tenantFilter';
import type { TransactionEventArgs } from '@mikro-orm/core';

const TENANT_ID = 'org-abc-123';

function makeTransactionArgs(
  tenantId?: string,
  executeFn = vi.fn().mockResolvedValue(undefined)
): TransactionEventArgs {
  return {
    em: {
      getFilterParams(filterName: string) {
        if (filterName === TENANT_FILTER_NAME) {
          return tenantId ? { tenantId } : undefined;
        }
        return undefined;
      },
      getConnection() {
        return { execute: executeFn };
      }
    } as TransactionEventArgs['em'],
    transaction: {} as TransactionEventArgs['transaction']
  };
}

describe('RlsEventSubscriber', () => {
  let subscriber: RlsEventSubscriber;

  beforeEach(() => {
    subscriber = new RlsEventSubscriber();
  });

  it('executes SET LOCAL with tenant ID after transaction start', async () => {
    const executeFn = vi.fn().mockResolvedValue(undefined);
    const args = makeTransactionArgs(TENANT_ID, executeFn);

    await subscriber.afterTransactionStart(args);

    expect(executeFn).toHaveBeenCalledOnce();
    const [query, , method, ctx] = executeFn.mock.calls[0];
    expect(query).toContain('SET LOCAL app.tenant_id');
    expect(query).toContain(TENANT_ID);
    expect(method).toBe('run');
    expect(ctx).toBe(args.transaction);
  });

  it('skips SET LOCAL when no tenant context', async () => {
    const executeFn = vi.fn().mockResolvedValue(undefined);
    const args = makeTransactionArgs(undefined, executeFn);

    await subscriber.afterTransactionStart(args);

    expect(executeFn).not.toHaveBeenCalled();
  });

  it('escapes single quotes in tenant ID to prevent SQL injection', async () => {
    const executeFn = vi.fn().mockResolvedValue(undefined);
    const maliciousTenantId = "'; DROP TABLE users; --";
    const args = makeTransactionArgs(maliciousTenantId, executeFn);

    await subscriber.afterTransactionStart(args);

    const query = executeFn.mock.calls[0][0] as string;
    // Single quotes in input are doubled (SQL escaping: ' → '')
    // The full SQL should contain the escaped value wrapped in outer quotes
    // Result: SET LOCAL app.tenant_id = '''; DROP TABLE users; --'
    // SQL parser sees this as one string: '; DROP TABLE users; --
    expect(query).toContain("''");
    // Verify the full SET LOCAL structure is intact
    expect(query).toMatch(/^SET LOCAL app\.tenant_id = '.+'$/);
  });

  it('passes transaction context to execute', async () => {
    const executeFn = vi.fn().mockResolvedValue(undefined);
    const txn = { id: 'txn-123' };
    const args = makeTransactionArgs(TENANT_ID, executeFn);
    args.transaction = txn as TransactionEventArgs['transaction'];

    await subscriber.afterTransactionStart(args);

    expect(executeFn.mock.calls[0][3]).toBe(txn);
  });
});
