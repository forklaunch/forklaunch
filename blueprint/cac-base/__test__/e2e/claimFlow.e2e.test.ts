/**
 * Real end-to-end tests: a real Postgres (testcontainers), the module's
 * actual migrations, actual entities, and the actual app — built and
 * listening exactly the way server.ts builds it (forklaunchExpress +
 * setupTenantFilter/setupRls + real surfacePermissions/surfaceRoles) —
 * exercised with real `fetch()` HTTP calls, not an in-process shortcut.
 * A real self-signed JWT and a tiny in-process IAM stub answer the same
 * auth chain a production request would go through, so this is real JWT
 * verification and real permission-gated access, not a bypass.
 *
 * (An in-process `.sdk` + `executeMiddlewares: true` approach was tried
 * first and hits a real, unrelated framework gap — forklaunchExpress's
 * response enrichment calls `res.getHeaders()`, which the `.sdk` path's
 * synthetic response object doesn't implement. Real HTTP sidesteps it
 * and is exactly what was already proven to work by hand against this
 * same server while building the demo.)
 *
 * This is exactly the kind of test that would have caught both entity
 * bugs found while building that manual demo (icd10Code's runtime
 * column-name mismatch, and dateOfBirth's wrong migration column type)
 * automatically, the first time either landed — a real DB round-trip
 * through the real migration, instead of only ScrubbingService exercised
 * against in-memory data.
 */
import {
  activateCptLicense,
  ALL_CAC_PERMISSIONS,
  cleanupTestDatabase,
  clearDatabase,
  forkPostgresEm,
  getClaimCodeSetType,
  seedEncounter,
  seedEncounterWithCharges,
  setTestPermissions,
  setupTestDatabase,
  signTestJwt,
  startTestServer,
  TEST_ORGANIZATION_ID,
  TestSetupResult
} from './test-utils';

async function call(
  baseUrl: string,
  path: string,
  opts: { method?: string; body?: unknown; token?: string } = {}
) {
  const res = await fetch(`${baseUrl}${path}`, {
    method: opts.method ?? 'GET',
    headers: {
      'Content-Type': 'application/json',
      ...(opts.token ? { Authorization: `Bearer ${opts.token}` } : {})
    },
    body: opts.body != null ? JSON.stringify(opts.body) : undefined
  });
  const text = await res.text();
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    body = text;
  }
  return { status: res.status, body };
}

describe('cac-base end-to-end (real Postgres + Redis via testcontainers)', () => {
  let setup: TestSetupResult;
  let baseUrl: string;
  let jwt: string;

  beforeAll(async () => {
    setup = await setupTestDatabase();
    baseUrl = await startTestServer();
    jwt = await signTestJwt();
  }, 120_000);

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30_000);

  beforeEach(async () => {
    await clearDatabase(setup);
    setTestPermissions(ALL_CAC_PERMISSIONS);
  });

  describe('build + scrub', () => {
    it('a matching diagnosis and procedure scrubs clean', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-CLEAN-001',
        icd10Code: 'J06.9', // Acute upper respiratory infection
        procedureCode: 'PROC-001' // Office Visit — matches per mockLcdCrosswalk.ts
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      expect(built.status).toBe(200);
      expect((built.body as { status: string }).status).toBe('draft');

      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });
      expect(scrubbed.status).toBe(200);
      expect(scrubbed.body).toEqual({ status: 'ready', denials: [] });
    });

    it('a mismatched diagnosis and procedure gets flagged', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-FLAGGED-001',
        icd10Code: 'Z00.00', // routine physical — does not justify PROC-001
        procedureCode: 'PROC-001'
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({
        status: 'denied',
        denials: [{ carcCode: 'CO-50', category: 'lcd_ncd' }]
      });
    });

    it('an unrealistic unit count gets flagged as an NCCI MUE violation', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-MUE-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001',
        units: 5 // MOCK_NCCI_MUE_CAPS['PROC-001'] is 1
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toMatchObject({
        status: 'denied',
        denials: expect.arrayContaining([
          expect.objectContaining({ category: 'ncci_mue' })
        ])
      });
    });

    it('two procedures that conflict under NCCI PTP get flagged together', async () => {
      const em = forkPostgresEm(setup);
      // PROC-001 + PROC-002 is a mock PTP conflict pair
      // (MOCK_NCCI_PTP_CONFLICTS) — each diagnosis justifies its own
      // procedure per the mock LCD/NCD crosswalk, so PTP is the only
      // finding this should produce.
      const encounterId = await seedEncounterWithCharges(em, {
        mrn: 'E2E-PTP-001',
        icd10Code: ['J06.9', 'Z00.00'],
        charges: [{ procedureCode: 'PROC-001' }, { procedureCode: 'PROC-002' }]
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({
        status: 'denied',
        denials: [{ carcCode: 'CO-97', category: 'ncci_ptp' }]
      });
    });

    it('procedures that do not conflict under NCCI PTP scrub clean', async () => {
      const em = forkPostgresEm(setup);
      // PROC-001 + PROC-003 is not a mock PTP conflict pair — the negative
      // case, proving the check is pair-specific and not "any two charges."
      const encounterId = await seedEncounterWithCharges(em, {
        mrn: 'E2E-PTP-NEGATIVE-001',
        icd10Code: ['J06.9', 'R73.09'],
        charges: [{ procedureCode: 'PROC-001' }, { procedureCode: 'PROC-003' }]
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({ status: 'ready', denials: [] });
    });

    it('a claim with no diagnosis codes gets flagged as required_fields', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounterWithCharges(em, {
        mrn: 'E2E-REQFIELDS-NODIAG-001',
        icd10Code: [], // no diagnoses on this encounter at all
        charges: [{ procedureCode: 'PROC-999' }] // no LCD/NCD crosswalk entry either
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({
        status: 'denied',
        denials: [{ carcCode: 'CO-16', category: 'required_fields' }]
      });
    });

    it('a charge line with an invalid unit count gets flagged as required_fields', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounterWithCharges(em, {
        mrn: 'E2E-REQFIELDS-UNITS-001',
        icd10Code: 'J06.9',
        charges: [{ procedureCode: 'PROC-001', units: 0 }]
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({
        status: 'denied',
        denials: [{ carcCode: 'CO-16', category: 'required_fields' }]
      });
    });

    it('a mistyped/nonexistent procedure code gets flagged as required_fields', async () => {
      const em = forkPostgresEm(setup);
      // Not in MOCK_PROCEDURE_CODES at all — none of NCCI PTP/MUE/LCD-NCD
      // catch this on their own (they only match a code against a table of
      // *known* codes; an unrecognized code just never matches, silently),
      // so this is ClaimService's own unknown-procedure-code check, not
      // ScrubbingService's.
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-REQFIELDS-UNKNOWNCODE-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-0001' // a plausible typo of PROC-001
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({
        status: 'denied',
        denials: [{ carcCode: 'CO-16', category: 'required_fields' }]
      });
    });

    it('a recognized procedure code with no crosswalk entry is not flagged as unrecognized', async () => {
      const em = forkPostgresEm(setup);
      // PROC-999 is a real entry in MOCK_PROCEDURE_CODES with no
      // MOCK_LCD_CROSSWALK/NCCI data behind it — proves the new check only
      // catches codes the code-set provider has never heard of, not every
      // code lacking a crosswalk entry.
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-REQFIELDS-KNOWNUNMAPPED-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-999'
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({ status: 'ready', denials: [] });
    });

    it('a mistyped/nonexistent ICD-10-CM diagnosis code gets flagged as required_fields', async () => {
      const em = forkPostgresEm(setup);
      // PROC-999 is recognized with no crosswalk entry (isolates this
      // finding from an unrelated LCD/NCD one) — the diagnosis code itself
      // is what's unrecognized here, checked against the real Icd10Code
      // reference table via CodeValidationService, not against any mock
      // procedure data.
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-REQFIELDS-UNKNOWNDIAG-001',
        icd10Code: 'NOT-A-REAL-ICD10-CODE',
        procedureCode: 'PROC-999'
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const scrubbed = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });

      expect(scrubbed.body).toEqual({
        status: 'denied',
        denials: [{ carcCode: 'CO-16', category: 'required_fields' }]
      });
    });
  });

  describe('denial worklist', () => {
    it('lists a flagged claim and resolves it', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-WORKLIST-001',
        icd10Code: 'Z00.00',
        procedureCode: 'PROC-001'
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      await call(baseUrl, `/claim/${claimId}/scrub`, { method: 'POST', token: jwt });

      const listed = await call(baseUrl, '/denial', { token: jwt });
      const denials = listed.body as Array<{
        id: string;
        claimId: string;
        carcCode: string;
        worklistStatus: string;
      }>;
      expect(denials).toHaveLength(1);
      expect(denials[0]).toMatchObject({
        claimId,
        carcCode: 'CO-50',
        worklistStatus: 'open'
      });

      const resolved = await call(baseUrl, `/denial/${denials[0].id}/resolve`, {
        method: 'POST',
        token: jwt
      });
      expect(resolved.status).toBe(200);

      const listedAgain = await call(baseUrl, '/denial', { token: jwt });
      expect(
        (listedAgain.body as Array<{ worklistStatus: string }>)[0].worklistStatus
      ).toBe('resolved');
    });
  });

  describe('analytics', () => {
    it('reports clean/denial rates across a clean and a flagged claim', async () => {
      const em = forkPostgresEm(setup);
      const cleanId = await seedEncounter(em, {
        mrn: 'E2E-ANALYTICS-CLEAN',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
      const flaggedId = await seedEncounter(em, {
        mrn: 'E2E-ANALYTICS-FLAGGED',
        icd10Code: 'Z00.00',
        procedureCode: 'PROC-001'
      });

      for (const encounterId of [cleanId, flaggedId]) {
        const built = await call(baseUrl, '/claim/build', {
          method: 'POST',
          body: { encounterId },
          token: jwt
        });
        const claimId = (built.body as { id: string }).id;
        await call(baseUrl, `/claim/${claimId}/scrub`, {
          method: 'POST',
          token: jwt
        });
      }

      const summary = await call(baseUrl, '/analytics/claims/summary', {
        token: jwt
      });

      expect(summary.body).toMatchObject({
        totalScrubbedClaims: 2,
        cleanClaimRate: 50,
        denialRate: 50,
        denialsByCategory: { lcd_ncd: 1 }
      });
    });
  });

  describe('auth', () => {
    it('rejects a request with no Authorization header', async () => {
      const result = await call(baseUrl, '/denial');
      expect(result.status).toBe(401);
    });

    it('rejects a valid token that lacks the required permission', async () => {
      setTestPermissions([]); // valid JWT, but IAM grants nothing
      const result = await call(baseUrl, '/denial', { token: jwt });
      expect(result.status).not.toBe(200);
    });
  });

  describe('tenant isolation', () => {
    it("never returns another organization's denials", async () => {
      const em = forkPostgresEm(setup);
      const otherOrgEncounterId = await seedEncounter(em, {
        mrn: 'E2E-OTHER-ORG-001',
        icd10Code: 'Z00.00',
        procedureCode: 'PROC-001',
        organizationId: '99999999-9999-9999-9999-999999999999'
      });

      const otherOrgJwt = await signTestJwt({
        organizationId: '99999999-9999-9999-9999-999999999999'
      });
      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId: otherOrgEncounterId },
        token: otherOrgJwt
      });
      const claimId = (built.body as { id: string }).id;
      await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: otherOrgJwt
      });

      const listedAsTestOrg = await call(baseUrl, '/denial', { token: jwt }); // TEST_ORGANIZATION_ID, not the other org
      expect(listedAsTestOrg.body).toEqual([]);
    });
  });

  describe('code-set feature gate (§5)', () => {
    it('describes the mock provider when no CPT license is active', async () => {
      const described = await call(baseUrl, '/codeSet', { token: jwt });
      expect(described.status).toBe(200);
      expect(described.body).toEqual({ codeSetType: 'mock', licensed: false });
    });

    it('describes the real CPT provider once the organization license is active', async () => {
      const em = forkPostgresEm(setup);
      await activateCptLicense(em);

      const described = await call(baseUrl, '/codeSet', { token: jwt });
      expect(described.body).toEqual({ codeSetType: 'cpt', licensed: true });
    });
  });

  describe('code-set cutover (§5)', () => {
    it('claims built before a license activation stay on mock after it, only new claims pick up CPT', async () => {
      const em = forkPostgresEm(setup);

      // 1. Build a claim under the organization's default (unlicensed) state.
      const encounterBefore = await seedEncounter(em, {
        mrn: 'E2E-CUTOVER-BEFORE-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
      const builtBefore = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId: encounterBefore },
        token: jwt
      });
      expect(
        (builtBefore.body as { codeSetType: string }).codeSetType
      ).toBe('mock');
      const claimBeforeId = (builtBefore.body as { id: string }).id;

      // 2. Flip the organization's CPT license to active.
      await activateCptLicense(em);

      // 3. A new encounter's claim, built after the flip, picks up CPT.
      const encounterAfter = await seedEncounter(em, {
        mrn: 'E2E-CUTOVER-AFTER-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
      const builtAfter = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId: encounterAfter },
        token: jwt
      });
      expect(
        (builtAfter.body as { codeSetType: string }).codeSetType
      ).toBe('cpt');

      // 4. The earlier claim is never retroactively recoded — still 'mock'.
      const codeSetTypeAfterFlip = await getClaimCodeSetType(
        forkPostgresEm(setup),
        claimBeforeId
      );
      expect(codeSetTypeAfterFlip).toBe('mock');
    });

    it('a license active for one organization never affects another organization\'s claims', async () => {
      const em = forkPostgresEm(setup);
      const otherOrgId = '88888888-8888-8888-8888-888888888888';

      await activateCptLicense(em, TEST_ORGANIZATION_ID);

      const otherOrgEncounter = await seedEncounter(em, {
        mrn: 'E2E-CUTOVER-OTHERORG-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001',
        organizationId: otherOrgId
      });
      const otherOrgJwt = await signTestJwt({ organizationId: otherOrgId });
      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId: otherOrgEncounter },
        token: otherOrgJwt
      });

      expect((built.body as { codeSetType: string }).codeSetType).toBe('mock');
    });
  });
});
