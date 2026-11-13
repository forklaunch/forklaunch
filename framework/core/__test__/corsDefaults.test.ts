import type { CorsOptions } from 'cors';
import { describe, expect, it } from 'vitest';
import {
  cors,
  LOCAL_DEVELOPMENT_ORIGIN,
  parseCorsOrigins,
  resolveCorsOptions,
  withoutWildcardCredentials
} from '../src/http/middleware/request/cors.middleware';

type Headers = Record<string, string | undefined>;

/**
 * Runs the real cors middleware for one request and returns the response
 * headers it set (lower-cased), as a browser would see them.
 */
function respond(
  options: CorsOptions,
  origin: string | undefined,
  method = 'GET'
): Headers {
  const headers: Headers = {};
  const req = {
    method,
    headers: {
      ...(origin ? { origin } : {}),
      ...(method === 'OPTIONS'
        ? { 'access-control-request-method': 'POST' }
        : {})
    }
  };
  const res = {
    statusCode: 200,
    setHeader: (name: string, value: string) => {
      headers[name.toLowerCase()] = String(value);
    },
    getHeader: (name: string) => headers[name.toLowerCase()],
    getHeaders: () => headers,
    end: () => {}
  };
  cors(options)(req as never, res as never, () => {});
  return headers;
}

/** What an app that passes no `cors` option gets, for a given environment. */
const defaults = (env: Record<string, string | undefined>) =>
  resolveCorsOptions(undefined, env);

const ALLOW_ORIGIN = 'access-control-allow-origin';
const ALLOW_CREDENTIALS = 'access-control-allow-credentials';

describe('CORS defaults', () => {
  describe('no cors option, nothing configured', () => {
    it('production: no origin is allowed and the wildcard is never returned', () => {
      const { options, warning } = defaults({ NODE_ENV: 'production' });
      for (const method of ['GET', 'POST', 'OPTIONS']) {
        for (const origin of [
          'https://app.example.com',
          'http://localhost:3000',
          'null',
          undefined
        ]) {
          const headers = respond(options, origin, method);
          expect(headers[ALLOW_ORIGIN]).toBeUndefined();
          expect(headers[ALLOW_CREDENTIALS]).toBeUndefined();
        }
      }
      expect(warning).toContain('CORS_ORIGINS');
    });

    it('outside production: only localhost origins are allowed, never the wildcard', () => {
      for (const env of [{}, { NODE_ENV: 'development' }, { NODE_ENV: 'test' }]) {
        const { options, warning } = defaults(env);
        for (const origin of [
          'http://localhost:3000',
          'http://localhost',
          'https://localhost:8443',
          'http://127.0.0.1:5173',
          'http://[::1]:3000'
        ]) {
          expect(respond(options, origin)[ALLOW_ORIGIN]).toBe(origin);
          expect(respond(options, origin, 'OPTIONS')[ALLOW_ORIGIN]).toBe(origin);
        }
        for (const origin of [
          'https://app.example.com',
          'http://localhost.evil.example',
          'http://localhost:3000.evil.example',
          'http://evil.example/http://localhost:3000',
          'http://127.0.0.1.evil.example',
          'http://notlocalhost:3000',
          'null',
          '*'
        ]) {
          expect(respond(options, origin)[ALLOW_ORIGIN]).toBeUndefined();
          expect(
            respond(options, origin, 'OPTIONS')[ALLOW_ORIGIN]
          ).toBeUndefined();
        }
        expect(respond(options, undefined)[ALLOW_ORIGIN]).toBeUndefined();
        expect(warning).toContain('CORS_ORIGINS');
      }
    });

    it('an empty CORS_ORIGINS counts as unset', () => {
      for (const value of ['', ' ', ',', ' , ']) {
        const { options, warning } = defaults({
          NODE_ENV: 'production',
          CORS_ORIGINS: value
        });
        expect(options.origin).toBe(false);
        expect(warning).toBeDefined();
      }
    });
  });

  describe('no cors option, CORS_ORIGINS set', () => {
    const env = {
      NODE_ENV: 'production',
      CORS_ORIGINS: 'https://app.example.com, https://admin.example.com/'
    };

    it('allows exactly the listed origins', () => {
      const { options, warning } = defaults(env);
      expect(warning).toBeUndefined();
      for (const origin of [
        'https://app.example.com',
        'https://admin.example.com'
      ]) {
        const headers = respond(options, origin);
        expect(headers[ALLOW_ORIGIN]).toBe(origin);
        expect(headers.vary).toContain('Origin');
        expect(respond(options, origin, 'OPTIONS')[ALLOW_ORIGIN]).toBe(origin);
      }
    });

    it('does not allow any other origin', () => {
      const { options } = defaults(env);
      for (const origin of [
        'https://evil.example.com',
        'https://app.example.com.evil.example',
        'http://app.example.com',
        'http://localhost:3000'
      ]) {
        expect(respond(options, origin)[ALLOW_ORIGIN]).toBeUndefined();
      }
    });

    it('applies in development too, replacing the localhost default', () => {
      const { options } = defaults({
        NODE_ENV: 'development',
        CORS_ORIGINS: 'https://app.example.com'
      });
      expect(respond(options, 'https://app.example.com')[ALLOW_ORIGIN]).toBe(
        'https://app.example.com'
      );
      expect(
        respond(options, 'http://localhost:3000')[ALLOW_ORIGIN]
      ).toBeUndefined();
    });

    it('a listed "*" is an explicit choice, and comes without credentials', () => {
      const { options, warning } = defaults({ CORS_ORIGINS: '*' });
      const headers = respond(options, 'https://anything.example.com');
      expect(headers[ALLOW_ORIGIN]).toBe('*');
      expect(headers[ALLOW_CREDENTIALS]).toBeUndefined();
      expect(warning).toBeUndefined();
    });
  });

  describe('the app passes cors options', () => {
    it('uses them exactly as given', () => {
      for (const appCors of [
        { origin: 'https://app.example.com', credentials: true },
        { origin: ['https://a.example.com'], methods: ['GET'] },
        { origin: true, credentials: true },
        { origin: false },
        { origin: '*' }
      ] as CorsOptions[]) {
        const { options, warning } = resolveCorsOptions(appCors, {
          NODE_ENV: 'production',
          CORS_ORIGINS: 'https://ignored.example.com'
        });
        expect(options).toBe(appCors);
        expect(warning).toBeUndefined();
      }
    });

    it('options without an origin (including an empty object) still allow every origin, and say so', () => {
      for (const appCors of [{}, { methods: ['GET'] }] as CorsOptions[]) {
        const { options, warning } = resolveCorsOptions(appCors, {
          NODE_ENV: 'production',
          CORS_ORIGINS: 'https://ignored.example.com'
        });
        expect(options).toBe(appCors);
        expect(respond(options, 'https://anything.example.com')[ALLOW_ORIGIN]).toBe(
          '*'
        );
        expect(warning).toContain('EVERY origin');
        expect(warning).toContain('cors: {}');
      }
    });

    it('a configured origin is allowed, with credentials when asked', () => {
      const { options } = resolveCorsOptions(
        { origin: ['https://app.example.com'], credentials: true },
        { NODE_ENV: 'production' }
      );
      const headers = respond(options, 'https://app.example.com');
      expect(headers[ALLOW_ORIGIN]).toBe('https://app.example.com');
      expect(headers[ALLOW_CREDENTIALS]).toBe('true');
      expect(
        respond(options, 'https://evil.example.com')[ALLOW_ORIGIN]
      ).toBeUndefined();
    });

    it('never emits the wildcard together with credentials', () => {
      for (const appCors of [
        { credentials: true },
        { origin: '*', credentials: true }
      ] as CorsOptions[]) {
        const { options, warning } = resolveCorsOptions(appCors, {
          NODE_ENV: 'production'
        });
        // The options are the app's; the middleware withholds the header.
        expect(options).toBe(appCors);
        expect(warning).toContain('credentials');
        expect(withoutWildcardCredentials(options).credentials).toBe(false);
        for (const method of ['GET', 'OPTIONS']) {
          const headers = respond(options, 'https://app.example.com', method);
          expect(headers[ALLOW_ORIGIN]).toBe('*');
          expect(headers[ALLOW_CREDENTIALS]).toBeUndefined();
        }
      }
    });

    it('never emits the wildcard with credentials when an origin callback answers "*"', () => {
      const answers: Record<string, string | boolean> = {
        'https://wild.example.com': '*',
        'https://named.example.com': 'https://named.example.com',
        'https://reflected.example.com': true,
        'https://denied.example.com': false
      };
      const options: CorsOptions = {
        credentials: true,
        origin: (origin, callback) => callback(null, answers[origin ?? ''])
      };
      const wild = respond(options, 'https://wild.example.com');
      expect(wild[ALLOW_ORIGIN]).toBe('*');
      expect(wild[ALLOW_CREDENTIALS]).toBeUndefined();

      // Named and reflected origins keep their credentials, as before.
      for (const origin of [
        'https://named.example.com',
        'https://reflected.example.com'
      ]) {
        const headers = respond(options, origin);
        expect(headers[ALLOW_ORIGIN]).toBe(origin);
        expect(headers[ALLOW_CREDENTIALS]).toBe('true');
      }
      const denied = respond(options, 'https://denied.example.com');
      expect(denied[ALLOW_ORIGIN]).toBeUndefined();
      expect(denied[ALLOW_CREDENTIALS]).toBeUndefined();
    });

    it('passes an origin callback error on', () => {
      const failure = new Error('lookup failed');
      let received: unknown;
      cors({
        credentials: true,
        origin: (_origin, callback) => callback(failure)
      })(
        { method: 'GET', headers: { origin: 'https://a.example.com' } } as never,
        { setHeader: () => {}, getHeader: () => undefined } as never,
        (error) => {
          received = error;
        }
      );
      expect(received).toBe(failure);
    });
  });

  it('parses the comma separated list', () => {
    expect(parseCorsOrigins(undefined)).toEqual([]);
    expect(
      parseCorsOrigins(' https://a.example.com/ ,https://b.example.com,,https://a.example.com')
    ).toEqual(['https://a.example.com', 'https://b.example.com']);
  });

  it('the localhost pattern is anchored', () => {
    expect(LOCAL_DEVELOPMENT_ORIGIN.test('http://localhost:3000')).toBe(true);
    expect(LOCAL_DEVELOPMENT_ORIGIN.test('http://localhost:3000/')).toBe(false);
    expect(LOCAL_DEVELOPMENT_ORIGIN.test('http://localhost@evil.example')).toBe(
      false
    );
    expect(LOCAL_DEVELOPMENT_ORIGIN.test('ftp://localhost')).toBe(false);
  });
});
