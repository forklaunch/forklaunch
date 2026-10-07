import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ForklaunchExpressLikeRouter,
  resetProcessHandlersForTesting
} from '../src/http/router/expressLikeRouter';

/**
 * Every router used to register four process-level listeners in its
 * constructor, so an application's listener count grew with its router count.
 * A service with eleven routers produced this at startup:
 *
 *   MaxListenersExceededWarning: Possible EventEmitter memory leak detected.
 *   11 unhandledRejection listeners added to [process].
 *
 * The warning was only the visible half. On one unhandled rejection every
 * handler ran — logging the same error once per router, each calling
 * process.exit(1).
 *
 * The constructor skips registration entirely under NODE_ENV=test / VITEST, so
 * these tests clear both to exercise the path a real service takes.
 */

const SIGNALS = [
  'uncaughtException',
  'unhandledRejection',
  'exit',
  'SIGINT'
] as const;

const collector = () =>
  ({
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn()
  }) as never;

const buildRouter = () =>
  new ForklaunchExpressLikeRouter(
    '/' as never,
    {} as never,
    { use: vi.fn() } as never,
    [] as never,
    collector(),
    undefined
  );

describe('router process handler registration', () => {
  let nodeEnv: string | undefined;
  let vitestFlag: string | undefined;
  let baseline: Record<string, number>;

  beforeEach(() => {
    nodeEnv = process.env.NODE_ENV;
    vitestFlag = process.env.VITEST;
    delete process.env.NODE_ENV;
    delete process.env.VITEST;
    resetProcessHandlersForTesting();
    baseline = Object.fromEntries(
      SIGNALS.map((s) => [s, process.listenerCount(s)])
    );
  });

  afterEach(() => {
    // Remove only what this test added, so suites stay independent.
    for (const signal of SIGNALS) {
      const listeners = process.listeners(signal);
      for (const listener of listeners.slice(baseline[signal])) {
        process.removeListener(signal, listener as never);
      }
    }
    if (nodeEnv === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = nodeEnv;
    if (vitestFlag === undefined) delete process.env.VITEST;
    else process.env.VITEST = vitestFlag;
    resetProcessHandlersForTesting();
  });

  it('registers one handler per signal for a single router', () => {
    buildRouter();
    for (const signal of SIGNALS) {
      expect(process.listenerCount(signal) - baseline[signal]).toBe(1);
    }
  });

  it('does not add more as routers multiply', () => {
    // Twelve is past Node's default MaxListeners of 10 — the count that
    // produced the production warning.
    for (let i = 0; i < 12; i++) {
      buildRouter();
    }

    for (const signal of SIGNALS) {
      expect(process.listenerCount(signal) - baseline[signal]).toBe(1);
    }
  });

  it('stays under Node’s MaxListeners for a realistic router count', () => {
    for (let i = 0; i < 12; i++) {
      buildRouter();
    }
    expect(
      process.listenerCount('unhandledRejection') - baseline.unhandledRejection
    ).toBeLessThan(process.getMaxListeners());
  });

  it('registers nothing under a test runner', () => {
    process.env.VITEST = 'true';
    resetProcessHandlersForTesting();
    const before = Object.fromEntries(
      SIGNALS.map((s) => [s, process.listenerCount(s)])
    );

    buildRouter();

    for (const signal of SIGNALS) {
      expect(process.listenerCount(signal)).toBe(before[signal]);
    }
  });

  it('logs an unhandled rejection exactly once, not once per router', () => {
    const seen: unknown[] = [];
    resetProcessHandlersForTesting();
    const first = {
      error: (message: unknown) => seen.push(message),
      warn: vi.fn(),
      info: vi.fn(),
      debug: vi.fn()
    } as never;

    new ForklaunchExpressLikeRouter(
      '/' as never,
      {} as never,
      { use: vi.fn() } as never,
      [] as never,
      first,
      undefined
    );
    for (let i = 0; i < 5; i++) {
      buildRouter();
    }

    const handlers = process
      .listeners('unhandledRejection')
      .slice(baseline.unhandledRejection);
    expect(handlers).toHaveLength(1);

    // Calling the surviving handler must not fan out; process.exit is stubbed
    // because the real handler ends the process.
    const exit = vi
      .spyOn(process, 'exit')
      .mockImplementation((() => undefined) as never);
    (handlers[0] as (reason: unknown) => void)('boom');
    exit.mockRestore();

    expect(seen).toEqual(['Unhandled rejection: boom']);
  });
});
