import {
    MockSchemaValidator,
    mockSchemaValidator
} from '@forklaunch/validator/tests/mockSchemaValidator';
import { JWTPayload, SignJWT, exportJWK, generateKeyPair } from 'jose';
import { beforeAll, describe, expect, it } from 'vitest';
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

describe('auth middleware - scope hierarchy validation', () => {
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
                error: () => { },
                debug: () => { }
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
            end: () => { },
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

    describe('scope hierarchy validation - FOR-27 bug fix', () => {
        /**
         * Test for the bug described in FOR-27:
         * Operator precedence error caused the scope hierarchy validation to return
         * array indices instead of booleans, allowing authorization bypass.
         *
         * Before fix: scopeHeirarchy?.indexOf(scope) ?? -1 > -1
         * Parsed as: scopeHeirarchy?.indexOf(scope) ?? ((-1 > -1))
         *
         * After fix: (scopeHeirarchy?.indexOf(scope) ?? -1) > -1
         * Properly checks if scope exists in hierarchy
         */
        it('should deny access when user has unauthorized scopes not in hierarchy', async () => {
            const token = await createSignedJWT({
                sub: 'user123',
                organizationId: 'org123'
            });

            // User scopes include 'delete' which is NOT in the hierarchy
            const surfaceScopes = async () => new Set(['read', 'delete']);

            const req = createMockRequest(
                token,
                {
                    jwt: { jwksPublicKeyUrl: jwksUrl },
                    requiredScope: 'read',
                    scopeHeirarchy: ['read', 'write', 'admin']
                },
                {
                    auth: {
                        surfaceScopes
                    }
                }
            );

            const res = createMockResponse();
            const next = () => { };

            await parseRequestAuth(req, res, next);

            // Should return 403 because 'delete' is not in the hierarchy
            expect(res.getStatus()).toBe(403);
            expect(res.getSentData()).toContain('Invalid scope');
        });

        it('should deny access when user has required scope but also has unauthorized scopes', async () => {
            const token = await createSignedJWT({
                sub: 'user123',
                organizationId: 'org123'
            });

            // User has required scope 'read' but also has 'admin' which is not defined in hierarchy
            const surfaceScopes = async () => new Set(['read', 'admin']);

            const req = createMockRequest(
                token,
                {
                    jwt: { jwksPublicKeyUrl: jwksUrl },
                    requiredScope: 'read',
                    scopeHeirarchy: ['read', 'write'] // 'admin' not in hierarchy
                },
                {
                    auth: {
                        surfaceScopes
                    }
                }
            );

            const res = createMockResponse();
            const next = () => { };

            await parseRequestAuth(req, res, next);

            // Should deny because 'admin' is not in the hierarchy
            expect(res.getStatus()).toBe(403);
            expect(res.getSentData()).toContain('Invalid scope');
        });

        it('should deny access when user does not have required scope', async () => {
            const token = await createSignedJWT({
                sub: 'user123',
                organizationId: 'org123'
            });

            // User does not have the required scope
            const surfaceScopes = async () => new Set(['write']);

            const req = createMockRequest(
                token,
                {
                    jwt: { jwksPublicKeyUrl: jwksUrl },
                    requiredScope: 'read',
                    scopeHeirarchy: ['read', 'write', 'admin']
                },
                {
                    auth: {
                        surfaceScopes
                    }
                }
            );

            const res = createMockResponse();
            const next = () => { };

            await parseRequestAuth(req, res, next);

            // Should deny because required scope is missing
            expect(res.getStatus()).toBe(403);
            expect(res.getSentData()).toContain('Invalid scope');
        });

        it('should handle empty user scopes', async () => {
            const token = await createSignedJWT({
                sub: 'user123',
                organizationId: 'org123'
            });

            const surfaceScopes = async () => new Set([]);

            const req = createMockRequest(
                token,
                {
                    jwt: { jwksPublicKeyUrl: jwksUrl },
                    requiredScope: 'read',
                    scopeHeirarchy: ['read', 'write', 'admin']
                },
                {
                    auth: {
                        surfaceScopes
                    }
                }
            );

            const res = createMockResponse();
            const next = () => { };

            await parseRequestAuth(req, res, next);

            // Should deny because required scope is missing
            expect(res.getStatus()).toBe(403);
            expect(res.getSentData()).toContain('Invalid scope');
        });

        it('should exploit scenario from bug report - unauthorized scopes should be denied', async () => {
            /**
             * This test demonstrates the exploit scenario from FOR-27:
             * 1. Attacker has valid token with required scope 'read'
             * 2. Token also contains unauthorized scopes 'delete' and 'admin'
             * 3. Scope hierarchy only defines ['read', 'write']
             * 4. With the fix, unauthorized scopes should be rejected
             */
            const token = await createSignedJWT({
                sub: 'attacker123',
                organizationId: 'org123'
            });

            // Attacker's scopes: has required 'read' + unauthorized 'delete' + 'admin'
            const surfaceScopes = async () => new Set(['read', 'delete', 'admin']);

            const req = createMockRequest(
                token,
                {
                    jwt: { jwksPublicKeyUrl: jwksUrl },
                    requiredScope: 'read',
                    scopeHeirarchy: ['read', 'write'] // Only 'read' and 'write' are authorized
                },
                {
                    auth: {
                        surfaceScopes
                    }
                }
            );

            const res = createMockResponse();
            const next = () => { };

            await parseRequestAuth(req, res, next);

            // With the fix, this should be DENIED because 'delete' and 'admin'
            // are not in the hierarchy
            expect(res.getStatus()).toBe(403);
            expect(res.getSentData()).toContain('Invalid scope');
        });
    });
});
