import { getEnvVar } from '@forklaunch/common';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FieldEncryptor,
  registerEncryptor,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import {
  BlueprintTestHarness,
  DatabaseType,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager } from '@mikro-orm/postgresql';
import dotenv from 'dotenv';
import { exportJWK, generateKeyPair, SignJWT } from 'jose';
import http from 'node:http';
import * as path from 'path';

export { TestSetupResult };

// `TestSetupResult.orm` is `@forklaunch/testing`'s driver-agnostic
// `AnyMikroORM` (the harness supports postgres/mysql/mongodb/etc.), so its
// `.em` is the generic `EntityManager<IDatabaseDriver<Connection>>` from
// `@mikro-orm/core` — missing the Postgres-only members (createQueryBuilder,
// qb, getKysely, ...) that a Postgres-specific `EntityManager` (from
// `@mikro-orm/postgresql`, e.g. `CodeSetLoaderService`'s constructor) needs.
// This suite always configures `databaseType: 'postgres'`, so the narrowing
// is safe; centralized here instead of an `as` at each of the ~20 call sites.
export function forkPostgresEm(setup: TestSetupResult): EntityManager {
  return setup.orm!.em.fork() as EntityManager;
}

dotenv.config({ path: path.join(__dirname, '../../.env.test') });

// Compliance entities (Patient, CodeSetLicense, ...) require a registered
// field encryptor before any entity module is imported.
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

export const TEST_ORGANIZATION_ID = '11111111-1111-1111-1111-111111111111';
export const TEST_SUB = '22222222-2222-2222-2222-222222222222';

// ---------------------------------------------------------------------------
// Self-signed JWT + JWKS (a `data:` URI, so no server is needed for the JWKS
// side at all) — the same real-JWT-verification technique proven against
// the live server while building the demo, ported into a proper test.
// ---------------------------------------------------------------------------

let signingKey: Awaited<ReturnType<typeof generateKeyPair>>['privateKey'];
let jwksDataUri: string;

async function ensureKeys() {
  if (jwksDataUri) return;
  const { publicKey, privateKey } = await generateKeyPair('RS256');
  signingKey = privateKey;
  const jwk = await exportJWK(publicKey);
  jwk.kid = 'e2e-test-key';
  jwk.alg = 'RS256';
  jwk.use = 'sig';
  jwksDataUri = `data:application/json,${encodeURIComponent(
    JSON.stringify({ keys: [jwk] })
  )}`;
}

export async function getJwksDataUri(): Promise<string> {
  await ensureKeys();
  return jwksDataUri;
}

export async function signTestJwt(
  claims: { organizationId?: string; sub?: string } = {}
): Promise<string> {
  await ensureKeys();
  const { organizationId = TEST_ORGANIZATION_ID, sub = TEST_SUB } = claims;
  return new SignJWT({ organizationId })
    .setProtectedHeader({ alg: 'RS256', kid: 'e2e-test-key' })
    .setSubject(sub)
    .setIssuedAt()
    .setExpirationTime('1h')
    .sign(signingKey);
}

// ---------------------------------------------------------------------------
// IAM stub — cac-base's real surfacePermissions/surfaceRoles cross-service
// call (via universalSdk) still makes a genuine HTTP request during the
// auth middleware chain, even for an in-process `.sdk` route call. This
// answers it for real, with a per-test-mutable permission/role set so
// access-denied paths can be tested too, not just the happy path.
// ---------------------------------------------------------------------------

let permissions: Array<{ slug: string }> = [];
let roles: Array<{ name: string }> = [];

export function setTestPermissions(slugs: string[]): void {
  permissions = slugs.map((slug) => ({ slug }));
}

export function setTestRoles(names: string[]): void {
  roles = names.map((name) => ({ name }));
}

export const ALL_CAC_PERMISSIONS = [
  'coder:manage_claims',
  'biller:view_denials',
  'biller:manage_denials',
  'admin:view_analytics',
  'admin:manage_codesets'
];

const OPENAPI_DOC = {
  latest: {
    openapi: '3.1.0',
    info: { title: 'iam-stub', version: '1' },
    paths: {
      '/{id}/surface-permissions': {
        get: {
          operationId: 'user.surfacePermissions',
          responses: {
            200: {
              content: {
                'application/json': {
                  schema: {
                    type: 'array',
                    items: {
                      type: 'object',
                      properties: { slug: { type: 'string' } },
                      required: ['slug']
                    }
                  }
                }
              }
            }
          }
        }
      },
      '/{id}/surface-roles': {
        get: {
          operationId: 'user.surfaceRoles',
          responses: {
            200: {
              content: {
                'application/json': {
                  schema: {
                    type: 'array',
                    items: {
                      type: 'object',
                      properties: { name: { type: 'string' } },
                      required: ['name']
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
};

let iamServer: http.Server | undefined;
let iamPort: number | undefined;

function sendJson(res: http.ServerResponse, status: number, body: unknown) {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(payload)
  });
  res.end(payload);
}

export async function startIamStub(): Promise<number> {
  iamServer = http.createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://localhost');
    if (url.pathname === '/api/v1/openapi-hash') {
      res.writeHead(200, { 'Content-Type': 'text/plain' });
      res.end('e2e-1');
      return;
    }
    if (url.pathname === '/api/v1/openapi') {
      sendJson(res, 200, OPENAPI_DOC);
      return;
    }
    if (/^\/[^/]+\/surface-permissions$/.test(url.pathname)) {
      sendJson(res, 200, permissions);
      return;
    }
    if (/^\/[^/]+\/surface-roles$/.test(url.pathname)) {
      sendJson(res, 200, roles);
      return;
    }
    res.writeHead(404).end();
  });

  await new Promise<void>((resolve) => iamServer!.listen(0, resolve));
  iamPort = (iamServer!.address() as { port: number }).port;
  return iamPort;
}

export async function stopIamStub(): Promise<void> {
  if (iamServer) {
    await new Promise<void>((resolve, reject) =>
      iamServer!.close((err) => (err ? reject(err) : resolve()))
    );
    iamServer = undefined;
  }
}

// ---------------------------------------------------------------------------
// Database harness
// ---------------------------------------------------------------------------

let harness: BlueprintTestHarness;

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  const port = await startIamStub();
  const jwks = await getJwksDataUri();
  setTestPermissions(ALL_CAC_PERMISSIONS);
  setTestRoles(['coder', 'biller', 'admin']);

  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../../mikro-orm.config');
      return { ...config, discovery: { ...config.discovery } };
    },
    databaseType: getEnvVar('DATABASE_TYPE') as DatabaseType,
    useMigrations: true,
    migrationsPath: path.join(__dirname, '../../migrations'),
    needsRedis: true,
    customEnvVars: {
      JWKS_PUBLIC_KEY_URL: jwks,
      IAM_URL: `http://localhost:${port}`,
      ENCRYPTION_KEY: '0'.repeat(64)
    }
  });

  const setup = await harness.setup();
  await seedIcd10ReferenceCodes(setup);
  return setup;
};

// ClaimService.scrubClaim validates every diagnosis code against the
// Icd10Code reference table and accepts any HCPCS code from HcpcsCode, so
// an empty table would deny every claim in this suite. Seeded through the
// real pipeline scripts/refresh-code-sets.ts runs: file text in the CDC
// codes-file layout (no dots, as CDC publishes them) and a HCPCS CSV,
// parsed and loaded by the same loaders. Only the codes this suite uses.
export const ICD10_CDC_SAMPLE = [
  'J069    Acute upper respiratory infection, unspecified',
  'Z0000   Encounter for general adult medical exam without abnormal findings',
  'R7309   Other abnormal glucose',
  'E119    Type 2 diabetes mellitus without complications'
].join('\n');
export const HCPCS_CSV_SAMPLE = [
  'code,description',
  'J3490,"Unclassified drugs"',
  'G0438,"Annual wellness visit, initial"'
].join('\n');

async function seedIcd10ReferenceCodes(setup: TestSetupResult): Promise<void> {
  if (!setup.orm) return;
  const { Readable } = await import('node:stream');
  const { CodeSetLoaderService } = await import(
    '../../persistence/etl/codeSetLoader.service'
  );
  const { loadIcd10Codes } = await import('../../persistence/etl/icd10.loader');
  const { loadHcpcsCodes } = await import('../../persistence/etl/hcpcs.loader');
  const loader = new CodeSetLoaderService(
    forkPostgresEm(setup),
    new OpenTelemetryCollector('test', 'info', {})
  );
  await loadIcd10Codes(loader, Readable.from([ICD10_CDC_SAMPLE]));
  await loadHcpcsCodes(loader, Readable.from([HCPCS_CSV_SAMPLE]));
}

// An HMAC Authorization header for an internal route. The server verifies
// the router-relative path (req.path), e.g. '/icd10/E11.9' for
// '/codeValidation/icd10/E11.9'.
export async function signHmac(method: string, routerPath: string): Promise<string> {
  const { generateHmacAuthHeaders } = await import('@forklaunch/core/http');
  const { ci, tokens } = await import('../../bootstrapper');
  return generateHmacAuthHeaders({
    secretKey: ci.resolve(tokens.HMAC_SECRET_KEY),
    method,
    path: routerPath
  }).authorization;
}

export const cleanupTestDatabase = async (): Promise<void> => {
  await stopTestServer();
  if (harness) {
    await harness.cleanup();
  }
  await stopIamStub();
};

// ---------------------------------------------------------------------------
// The real app — mirrors server.ts exactly: forklaunchExpress + real
// surfacePermissions/surfaceRoles + setupTenantFilter/setupRls, actually
// listening on a real port. Calling a bare router's own `.sdk` directly
// (without ever constructing the app via forklaunchExpress) skips the auth
// config entirely: surfacePermissions/surfaceRoles are wired onto the app's
// routerOptions by forklaunchExpress, not onto an unmounted router, so
// allowedPermissions has nothing to check against and every request fails
// auth regardless of how valid the JWT is. setupTenantFilter/setupRls are
// the same story for tenant isolation — both are only ever called in
// server.ts, so a test that never calls them isn't exercising real tenant
// scoping at all.
//
// `.sdk` + `executeMiddlewares: true` against a real forklaunchExpress app
// turns out not to be a viable combination today: the app's own response
// enrichment (added by forklaunchExpress, wrapping every res.json/res.send)
// calls res.getHeaders() unconditionally, which the `.sdk` path's synthetic
// response object never implements — a real gap in the framework's own
// testing utility, not anything in cac-base. A real listening server and
// real fetch() calls sidestep it entirely and are exactly what was already
// proven to work by hand against this same server earlier — so that's the
// approach here instead.
let testServer: { close: (cb: () => void) => void } | undefined;
const TEST_PORT = 18453;
export const TEST_BASE_URL = `http://localhost:${TEST_PORT}`;

export async function startTestServer(): Promise<string> {
  const { createAuthCacheService, forklaunchExpress, schemaValidator } =
    await import('@forklaunch/blueprint-core');
  const { setupRls, setupTenantFilter } = await import(
    '@forklaunch/core/persistence'
  );
  const { ci, tokens } = await import('../../bootstrapper');
  const { createSurfacePermissions, createSurfaceRoles } = await import(
    '../../surfacing'
  );
  const { analyticsRouter } = await import('../../api/routes/analytics.routes');
  const { claimRouter } = await import('../../api/routes/claim.routes');
  const { codeSetRouter } = await import('../../api/routes/codeSet.routes');
  const { codeValidationRouter } = await import(
    '../../api/routes/codeValidation.routes'
  );
  const { complianceRouter } = await import(
    '../../api/routes/compliance.routes'
  );
  const { denialRouter } = await import('../../api/routes/denial.routes');

  const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
  const orm = ci.resolve(tokens.Orm);
  setupTenantFilter(orm, { logger: openTelemetryCollector });
  setupRls(orm, { logger: openTelemetryCollector });

  const iamUrl = ci.resolve(tokens.IAM_URL);
  const hmacSecretKey = ci.resolve(tokens.HMAC_SECRET_KEY);
  const authCacheService = createAuthCacheService(ci.resolve(tokens.TtlCache));

  const app = forklaunchExpress(schemaValidator, openTelemetryCollector, {
    auth: {
      surfacePermissions: createSurfacePermissions({
        authCacheService,
        iamUrl,
        hmacSecretKey
      }),
      surfaceRoles: createSurfaceRoles({ authCacheService, iamUrl, hmacSecretKey })
    }
  });

  app.use(analyticsRouter);
  app.use(claimRouter);
  app.use(codeSetRouter);
  app.use(codeValidationRouter);
  app.use(complianceRouter);
  app.use(denialRouter);

  await new Promise<void>((resolve) => {
    const server = app.listen(TEST_PORT, 'localhost', () => resolve());
    testServer = server;
  });

  return TEST_BASE_URL;
}

export async function stopTestServer(): Promise<void> {
  if (testServer) {
    await new Promise<void>((resolve) => testServer!.close(() => resolve()));
    testServer = undefined;
  }
}

export const clearDatabase = async (setup: TestSetupResult): Promise<void> => {
  if (!setup.orm) return;
  const em = setup.orm.em.fork();
  // Declaration order isn't FK-dependency order — same reasoning as the
  // shared harness's own clearTestDatabase, but scoped to cac-base's tables
  // (avoids the compliance/retention entities' own uninitialized-metadata
  // quirks under a bare clearTestDatabase() call).
  const tables = [
    'denial',
    'remittance',
    'claim',
    'charge',
    'diagnosis',
    'encounter',
    'insurance',
    'patient',
    'code_set_license',
    'cpt_code'
  ];
  for (const table of tables) {
    await em.getConnection().execute(`truncate table "${table}" cascade`);
  }
  if (setup.redis) {
    await setup.redis.flushall();
  }
};

// ---------------------------------------------------------------------------
// Seed helpers — the module has no API to create a Patient/Encounter/
// Diagnosis/Charge (by design, that's the hospital's own EHR's job), so
// tests write them directly via the ORM, the same way a real integration
// would before ever calling cac-base.
// ---------------------------------------------------------------------------

export async function seedEncounter(
  em: EntityManager,
  opts: {
    mrn: string;
    icd10Code: string;
    procedureCode: string;
    units?: number;
    organizationId?: string;
  }
): Promise<string> {
  return seedEncounterWithCharges(em, {
    mrn: opts.mrn,
    icd10Code: opts.icd10Code,
    organizationId: opts.organizationId,
    charges: [{ procedureCode: opts.procedureCode, units: opts.units }]
  });
}

// Same as seedEncounter, but takes multiple charge lines on one encounter —
// needed for NCCI PTP, which only fires when two procedure codes land on
// the same claim (a single-charge encounter can never trigger it).
export async function seedEncounterWithCharges(
  em: EntityManager,
  opts: {
    mrn: string;
    icd10Code: string | string[];
    charges: Array<{ procedureCode: string; units?: number }>;
    organizationId?: string;
  }
): Promise<string> {
  const { Patient } = await import('../../persistence/entities/patient.entity');
  const { Encounter } = await import(
    '../../persistence/entities/encounter.entity'
  );
  const { Diagnosis } = await import(
    '../../persistence/entities/diagnosis.entity'
  );
  const { Charge } = await import('../../persistence/entities/charge.entity');

  const organizationId = opts.organizationId ?? TEST_ORGANIZATION_ID;
  // Patient.dateOfBirth is PHI (EncryptedType), derived from the master key
  // + tenantId (framework/core/src/persistence/fieldEncryptor.ts). Writing
  // it through a plain, unwrapped em would encrypt under the "no tenant"
  // (empty-string) key — permanently unreadable once the real controllers
  // (which now pass organizationId as tenantId, see claim.controller.ts)
  // try to decrypt it with the real per-org key.
  em = wrapEmWithTenantContext(em, organizationId) as EntityManager;

  const patient = em.create(Patient, {
    organizationId,
    mrn: opts.mrn,
    firstName: 'Test',
    lastName: 'Patient',
    dateOfBirth: new Date('1990-01-01')
  });

  const encounter = em.create(Encounter, {
    organizationId,
    patient,
    providerId: '33333333-3333-3333-3333-333333333333',
    visitDate: new Date()
  });

  const icd10Codes = Array.isArray(opts.icd10Code)
    ? opts.icd10Code
    : [opts.icd10Code];
  for (const icd10Code of icd10Codes) {
    em.create(Diagnosis, {
      organizationId,
      encounter,
      icd10Code
    });
  }

  for (const charge of opts.charges) {
    em.create(Charge, {
      organizationId,
      encounter,
      procedureCode: charge.procedureCode,
      units: charge.units ?? 1,
      amount: 125.0
    });
  }

  // Two statements, not `em.persist(...).flush()` — `persist()` returns
  // `this` from the *unproxied* target (Reflect.get + .apply preserves the
  // original identity), so a chained `.flush()` would silently run outside
  // wrapEmWithTenantContext's per-call `withEncryptionContext` wrapping,
  // falling back to the fragile ambient AsyncLocalStorage seed alone —
  // exactly the pooled-connection edge case tenantEm.ts's own doc comment
  // warns about. Calling `.flush()` directly on `em` keeps it on the proxy.
  em.persist([patient, encounter]);
  await em.flush();
  return encounter.id;
}

// Flips an organization's real-CPT feature gate on (§5) — the same
// CodeSetLicense row CodeSetProviderResolver looks up directly, no cross-
// service call. No cpt_code rows are needed alongside it: buildClaim only
// calls resolve() + describe(), never lookupProcedureCode(), so CptCodeProvider
// is never asked to actually look anything up in these tests.
export async function activateCptLicense(
  em: EntityManager,
  organizationId: string = TEST_ORGANIZATION_ID
): Promise<void> {
  const { CodeSetLicense } = await import(
    '../../persistence/entities/codeSetLicense.entity'
  );
  const { CodeSetType } = await import('../../domain/enum/codeSetType.enum');
  const { LicenseStatus } = await import('../../domain/enum/licenseStatus.enum');

  // A fresh, tenant-scoped EntityManager: the caller's em may still hold a
  // Patient from seeding, which flushing on a raw em would re-encrypt under
  // the wrong key (UnboundTenantError on core 3).
  const scoped = wrapEmWithTenantContext(em.fork(), organizationId) as EntityManager;
  const license = scoped.create(CodeSetLicense, {
    organizationId,
    codeSetType: CodeSetType.CPT,
    status: LicenseStatus.ACTIVE,
    signedAt: new Date()
  });
  scoped.persist(license);
  await scoped.flush();
}

// Reads a claim's codeSetType directly off the DB — buildClaim's response
// already returns it too, but this is how a *previously built* claim's
// stored value is checked after a later CodeSetLicense flip, since there's
// no GET /claim/:id endpoint to ask the API for it.
export async function getClaimCodeSetType(
  em: EntityManager,
  claimId: string
): Promise<string> {
  const { Claim } = await import('../../persistence/entities/claim.entity');
  const claim = await em.findOneOrFail(Claim, { id: claimId });
  return claim.codeSetType;
}
