import { describe, expect, it, vi } from 'vitest';
import { logBetterAuthApiError } from '../domain/utils/betterAuthErrorLogging.util';

const buildCollector = () => ({
  error: vi.fn(),
  warn: vi.fn(),
  info: vi.fn(),
  debug: vi.fn()
});

/** The shape a MikroORM/Postgres failure actually arrives in: wrapped. */
const buildDriverError = () => {
  const driver = Object.assign(
    new Error('null value in column "issuer" violates not-null constraint'),
    {
      code: '23502',
      table: 'account',
      column: 'issuer',
      severity: 'ERROR',
      // Postgres puts the offending ROW in `detail` — an email address, here.
      detail: 'Failing row contains (1, someone@example.com, null).',
      where: 'SQL statement "insert into account"'
    }
  );
  return Object.assign(new Error('insert into "account" failed'), {
    cause: driver
  });
};

describe('logBetterAuthApiError', () => {
  it('records the structural database fields an on-call engineer needs', () => {
    const otel = buildCollector();
    logBetterAuthApiError(otel as never, buildDriverError());

    expect(otel.error).toHaveBeenCalledTimes(1);
    const [message, meta] = otel.error.mock.calls[0];
    expect(message).toBe('[IAM] Better Auth API error');
    expect(meta).toMatchObject({
      code: '23502',
      table: 'account',
      column: 'issuer',
      severity: 'ERROR'
    });
    expect(meta.message).toContain('insert into "account" failed');
  });

  it('never records the Postgres fields that quote user data back', () => {
    const otel = buildCollector();
    logBetterAuthApiError(otel as never, buildDriverError());

    const serialised = JSON.stringify(otel.error.mock.calls[0]);
    // This is the whole reason the field allow-list exists. IAM errors carry
    // email addresses in `detail`, and CloudWatch is not where they belong.
    expect(serialised).not.toContain('someone@example.com');
    expect(serialised).not.toContain('Failing row contains');
    expect(serialised).not.toContain('SQL statement');
  });

  it('finds fields nested several causes deep', () => {
    const otel = buildCollector();
    const deep = Object.assign(new Error('outer'), {
      cause: Object.assign(new Error('middle'), {
        cause: Object.assign(new Error('inner'), { code: '23505' })
      })
    });

    logBetterAuthApiError(otel as never, deep);
    expect(otel.error.mock.calls[0][1]).toMatchObject({ code: '23505' });
  });

  it('does not spin on a cyclic cause chain', () => {
    const otel = buildCollector();
    const a: Record<string, unknown> = { message: 'a' };
    const b: Record<string, unknown> = { message: 'b', cause: a };
    a.cause = b;

    logBetterAuthApiError(otel as never, a);
    expect(otel.error).toHaveBeenCalledTimes(1);
  });

  it('truncates a long stack rather than shipping the whole thing', () => {
    const otel = buildCollector();
    const err = new Error('boom');
    err.stack = 'x'.repeat(50_000);

    logBetterAuthApiError(otel as never, err);
    expect(otel.error.mock.calls[0][1].stack.length).toBeLessThanOrEqual(2000);
  });

  it('handles a thrown non-Error without losing it', () => {
    const otel = buildCollector();
    logBetterAuthApiError(otel as never, 'plain string failure');

    expect(otel.error.mock.calls[0][1]).toMatchObject({
      message: 'plain string failure'
    });
  });

  it('records the endpoint when the context carries one', () => {
    const otel = buildCollector();
    logBetterAuthApiError(otel as never, new Error('boom'), {
      context: { path: '/sign-up/email' }
    });

    expect(otel.error.mock.calls[0][1]).toMatchObject({
      path: '/sign-up/email'
    });
  });

  it('never throws, even when the collector itself does', () => {
    const otel = buildCollector();
    otel.error.mockImplementation(() => {
      throw new Error('collector down');
    });

    // It runs inside Better Auth's error path: throwing here would replace a
    // diagnosable 500 with an undiagnosable one.
    expect(() =>
      logBetterAuthApiError(otel as never, new Error('boom'))
    ).not.toThrow();
  });
});
