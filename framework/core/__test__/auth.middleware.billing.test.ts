import {
  MockSchemaValidator,
  mockSchemaValidator
} from '@forklaunch/validator/tests/mockSchemaValidator';
import { JWTPayload, SignJWT, exportJWK, generateKeyPair } from 'jose';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  ForklaunchRequest,
  ForklaunchResponse,
  HttpContractDetails,
  MetricsDefinition,
  OpenTelemetryCollector,
  ParamsDictionary,
  RequestContext,
  SessionObject
} from '../src/http';
import { parseRequestAuth } from '../src/http/middleware/request/auth.middleware';
import { ExpressLikeRouterOptions } from '../src/http/types/expressLikeOptions';

describe('auth middleware - billing features', () => {
  let privateKey: CryptoKey;
  let publicKey: CryptoKey;
  let jwksUrl: string;

  beforeAll(async () => {
    // Generate keypair for JWT signing
    const keyPair = await generateKeyPair('RS256');
    privateKey = keyPair.privateKey;
    publicKey = keyPair.publicKey;

    // Export public key as JWK
    const jwk = await exportJWK(publicKey);
    jwksUrl = `data:application/json,${encodeURIComponent(
      JSON.stringify({ keys: [{ ...jwk, kid: 'test-key', alg: 'RS256' }] })
    )}`;
  });

  async function createSignedJWT(
    payload: Record<string, unknown>
  ): Promise<string> {
    return new SignJWT(payload)
      .setProtectedHeader({ alg: 'RS256', kid: 'test-key' })
      .setIssuedAt()
      .setExpirationTime('1h')
      .sign(privateKey);
  }

  /** Everything the middleware logged through the collector, per test. */
  let logged: unknown[][] = [];
  beforeEach(() => {
    logged = [];
  });

  function createMockRequest(
    token?: string,
    contractAuth?: Record<string, unknown>,
    globalAuth?: Record<string, unknown>
  ): ForklaunchRequest<
    MockSchemaValidator,
    ParamsDictionary,
    Record<string, unknown>,
    Record<string, string>,
    Record<string, string>,
    never,
    Record<string, unknown>
  > {
    return {
      path: '/test',
      originalPath: '/test',
      method: 'POST',
      context: {} as RequestContext,
      contractDetails: {
        name: 'Test',
        summary: 'Test',
        auth: contractAuth
      } as HttpContractDetails<MockSchemaValidator>,
      schemaValidator: mockSchemaValidator,
      params: {},
      headers: token ? { authorization: `Bearer ${token}` } : {},
      body: {},
      query: {},
      requestSchema: {},
      openTelemetryCollector: {
        error: (...args: unknown[]) => logged.push(args),
        warn: (...args: unknown[]) => logged.push(args),
        debug: () => {}
      } as unknown as OpenTelemetryCollector<MetricsDefinition>,
      version: {} as never,
      session: {} as JWTPayload,
      _parsedVersions: 0,
      _globalOptions: () =>
        (globalAuth || {}) as ExpressLikeRouterOptions<
          MockSchemaValidator,
          SessionObject<MockSchemaValidator>
        >
    };
  }

  function createMockResponse() {
    let statusCode = 200;
    let sentData: unknown = null;

    const res: ForklaunchResponse<
      unknown,
      Record<number, unknown>,
      Record<string, unknown>,
      Record<string, unknown>,
      never
    > & {
      getStatus: () => number;
      getSentData: () => unknown;
    } = {
      bodyData: null,
      statusCode: 200,
      headersSent: false,
      metricRecorded: false,
      getHeaders: () => ({ 'x-correlation-id': 'test-correlation-id' }),
      getHeader: () => undefined,
      setHeader: () => res,
      on: () => res,
      end: () => {},
      type: () => res,
      locals: {},
      cors: false,
      responseSchemas: {
        headers: {},
        responses: {}
      },
      sent: false,
      version: {} as never,
      status: function (code: number) {
        statusCode = code;
        res.statusCode = code;
        return {
          json: (data: unknown) => {
            sentData = data;
            res.bodyData = data;
            return true;
          },
          jsonp: () => true,
          send: (data: unknown) => {
            sentData = data;
            res.bodyData = data;
            return true;
          },
          sseEmitter: () => Promise.resolve(),
          type: () => res
        };
      },
      getStatus: () => statusCode,
      getSentData: () => sentData
    };

    return res;
  }

  describe('requireActiveSubscription', () => {
    it('should allow request when subscription exists and requireActiveSubscription is true', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceSubscription = async () => ({
        subscriptionId: 'sub123',
        planId: 'pro',
        planName: 'Pro',
        status: 'active',
        currentPeriodEnd: new Date(Date.now() + 86400000) // Tomorrow
      });

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requireActiveSubscription: true,
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceSubscription
          }
        }
      );

      const res = createMockResponse();
      let nextCalled = false;
      const next = () => {
        nextCalled = true;
      };

      await parseRequestAuth(req, res, next);

      expect(nextCalled).toBe(true);
      expect(res.getStatus()).toBe(200);
    });

    it('should reject request when subscription is null and requireActiveSubscription is true', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceSubscription = async () => null;

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requireActiveSubscription: true,
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceSubscription
          }
        }
      );

      const res = createMockResponse();
      const next = () => {};

      await parseRequestAuth(req, res, next);

      expect(res.getStatus()).toBe(403);
      expect(res.getSentData()).toContain('Active subscription required');

      // The failure is logged, but the credential itself never is; only a
      // fingerprint and the claims the token asserted.
      const serialized = JSON.stringify(logged);
      expect(logged.length).toBeGreaterThan(0);
      expect(serialized).not.toContain(token);
      expect(serialized).toContain('"tokenFingerprint"');
      expect(serialized).toContain('"sub":"user123"');
    });

    it('logs a reason and no credential when the JWT signature is invalid', async () => {
      const token = (await createSignedJWT({ sub: 'user123' })).replace(
        /.$/,
        (c) => (c === 'a' ? 'b' : 'a')
      );

      const req = createMockRequest(token, {
        jwt: { jwksPublicKeyUrl: jwksUrl },
        allowedRoles: new Set(['user'])
      });

      const res = createMockResponse();
      await parseRequestAuth(req, res, () => {});

      expect(res.getStatus()).toBe(403);
      const serialized = JSON.stringify(logged);
      expect(serialized).not.toContain(token);
      expect(serialized).toContain('"reason":"jwt_bad_signature"');
    });

    it('should reject request when surfaceSubscription is not provided but required', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requireActiveSubscription: true,
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user'])
            // surfaceSubscription missing
          }
        }
      );

      const res = createMockResponse();
      const next = () => {};

      await parseRequestAuth(req, res, next);

      expect(res.getStatus()).toBe(500);
      expect(res.getSentData()).toContain(
        'No subscription surfacing function provided'
      );
    });

    it('should allow request when requireActiveSubscription is not set', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          allowedRoles: new Set(['user'])
          // requireActiveSubscription not set
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user'])
          }
        }
      );

      const res = createMockResponse();
      let nextCalled = false;
      const next = () => {
        nextCalled = true;
      };

      await parseRequestAuth(req, res, next);

      expect(nextCalled).toBe(true);
      expect(res.getStatus()).toBe(200);
    });
  });

  describe('requiredFeatures', () => {
    it('should allow request when all required features are available', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceFeatures = async () =>
        new Set(['advanced_observability', 'custom_domains', 'multi_region']);

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requiredFeatures: ['advanced_observability', 'custom_domains'],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceFeatures
          }
        }
      );

      const res = createMockResponse();
      let nextCalled = false;
      const next = () => {
        nextCalled = true;
      };

      await parseRequestAuth(req, res, next);

      expect(nextCalled).toBe(true);
      expect(res.getStatus()).toBe(200);
    });

    it('should reject request when required features are missing', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceFeatures = async () => new Set(['advanced_observability']);

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requiredFeatures: [
            'advanced_observability',
            'custom_domains',
            'multi_region'
          ],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceFeatures
          }
        }
      );

      const res = createMockResponse();
      const next = () => {};

      await parseRequestAuth(req, res, next);

      expect(res.getStatus()).toBe(403);
      expect(res.getSentData()).toContain('Required features not available');
    });

    it('should reject request when surfaceFeatures is not provided but required', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requiredFeatures: ['advanced_observability'],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user'])
            // surfaceFeatures missing
          }
        }
      );

      const res = createMockResponse();
      const next = () => {};

      await parseRequestAuth(req, res, next);

      expect(res.getStatus()).toBe(500);
      expect(res.getSentData()).toContain(
        'No features surfacing function provided'
      );
    });

    it('should allow request when requiredFeatures is not set', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          allowedRoles: new Set(['user'])
          // requiredFeatures not set
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user'])
          }
        }
      );

      const res = createMockResponse();
      let nextCalled = false;
      const next = () => {
        nextCalled = true;
      };

      await parseRequestAuth(req, res, next);

      expect(nextCalled).toBe(true);
      expect(res.getStatus()).toBe(200);
    });

    it('should allow request when requiredFeatures is empty array', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requiredFeatures: [],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user'])
          }
        }
      );

      const res = createMockResponse();
      let nextCalled = false;
      const next = () => {
        nextCalled = true;
      };

      await parseRequestAuth(req, res, next);

      expect(nextCalled).toBe(true);
      expect(res.getStatus()).toBe(200);
    });
  });

  describe('combined checks', () => {
    it('should allow request when both subscription and features are valid', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceSubscription = async () => ({
        subscriptionId: 'sub123',
        planId: 'enterprise',
        planName: 'Enterprise',
        status: 'active',
        currentPeriodEnd: new Date(Date.now() + 86400000)
      });

      const surfaceFeatures = async () =>
        new Set(['advanced_observability', 'custom_domains']);

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requireActiveSubscription: true,
          requiredFeatures: ['advanced_observability'],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceSubscription,
            surfaceFeatures
          }
        }
      );

      const res = createMockResponse();
      let nextCalled = false;
      const next = () => {
        nextCalled = true;
      };

      await parseRequestAuth(req, res, next);

      expect(nextCalled).toBe(true);
      expect(res.getStatus()).toBe(200);
    });

    it('should reject when subscription is valid but features are missing', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceSubscription = async () => ({
        subscriptionId: 'sub123',
        planId: 'pro',
        planName: 'Pro',
        status: 'active',
        currentPeriodEnd: new Date(Date.now() + 86400000)
      });

      const surfaceFeatures = async () => new Set(['advanced_observability']);

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requireActiveSubscription: true,
          requiredFeatures: ['advanced_observability', 'multi_region'],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceSubscription,
            surfaceFeatures
          }
        }
      );

      const res = createMockResponse();
      const next = () => {};

      await parseRequestAuth(req, res, next);

      expect(res.getStatus()).toBe(403);
      expect(res.getSentData()).toContain('Required features not available');
    });

    it('should reject when features are valid but subscription is missing', async () => {
      const token = await createSignedJWT({
        sub: 'user123',
        organizationId: 'org123'
      });

      const surfaceSubscription = async () => null;

      const surfaceFeatures = async () =>
        new Set(['advanced_observability', 'custom_domains']);

      const req = createMockRequest(
        token,
        {
          jwt: { jwksPublicKeyUrl: jwksUrl },
          requireActiveSubscription: true,
          requiredFeatures: ['advanced_observability'],
          allowedRoles: new Set(['user'])
        },
        {
          auth: {
            surfaceRoles: async () => new Set(['user']),
            surfaceSubscription,
            surfaceFeatures
          }
        }
      );

      const res = createMockResponse();
      const next = () => {};

      await parseRequestAuth(req, res, next);

      expect(res.getStatus()).toBe(403);
      expect(res.getSentData()).toContain('Active subscription required');
    });
  });
});
