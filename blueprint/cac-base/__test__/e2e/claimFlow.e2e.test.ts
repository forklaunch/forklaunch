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
  setTestRoles,
  setupTestDatabase,
  signHmac,
  signTestJwt,
  startTestServer,
  TEST_ORGANIZATION_ID,
  TestSetupResult
} from './test-utils';

async function call(
  baseUrl: string,
  path: string,
  opts: { method?: string; body?: unknown; token?: string; authorization?: string } = {}
) {
  const res = await fetch(`${baseUrl}${path}`, {
    method: opts.method ?? 'GET',
    headers: {
      'Content-Type': 'application/json',
      ...(opts.token ? { Authorization: `Bearer ${opts.token}` } : {}),
      ...(opts.authorization ? { Authorization: opts.authorization } : {})
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

  describe('claim lifecycle', () => {
    it('re-scrubbing a claim replaces stale OPEN denials instead of duplicating them', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-RESCRUB-001',
        icd10Code: 'Z00.00', // does not justify PROC-001 — denied both times
        procedureCode: 'PROC-001'
      });

      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;

      const firstScrub = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });
      expect(
        (firstScrub.body as { denials: unknown[] }).denials
      ).toHaveLength(1);

      const secondScrub = await call(baseUrl, `/claim/${claimId}/scrub`, {
        method: 'POST',
        token: jwt
      });
      expect(
        (secondScrub.body as { denials: unknown[] }).denials
      ).toHaveLength(1);

      const worklist = await call(baseUrl, '/denial', { token: jwt });
      const forThisClaim = (
        worklist.body as Array<{ claimId: string }>
      ).filter((denial) => denial.claimId === claimId);
      expect(forThisClaim).toHaveLength(1);
    });

    it('rejects a second claim built from the same encounter', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-DUPCLAIM-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });

      const first = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      expect(first.status).toBe(200);

      const second = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      expect(second.status).toBe(409);
    });

    it('returns 404 building a claim from a nonexistent encounter', async () => {
      const result = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId: '00000000-0000-0000-0000-000000000000' },
        token: jwt
      });
      expect(result.status).toBe(404);
    });

    it('returns 404 scrubbing a nonexistent claim', async () => {
      const result = await call(
        baseUrl,
        '/claim/00000000-0000-0000-0000-000000000000/scrub',
        { method: 'POST', token: jwt }
      );
      expect(result.status).toBe(404);
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

    it('rejects a date range where since is after until', async () => {
      const result = await call(
        baseUrl,
        '/analytics/claims/summary?since=2026-09-12&until=2026-09-01',
        { token: jwt }
      );
      expect(result.status).toBe(400);
    });

    it('treats a date-only "until" as inclusive of that whole day', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-ANALYTICS-DATEONLY',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
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

      const today = new Date().toISOString().slice(0, 10); // "YYYY-MM-DD"
      const summary = await call(
        baseUrl,
        `/analytics/claims/summary?until=${today}`,
        { token: jwt }
      );

      expect(
        (summary.body as { totalScrubbedClaims: number }).totalScrubbedClaims
      ).toBeGreaterThanOrEqual(1);
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

    // Regression test for Patient.mrn's uniqueness: an MRN is only unique
    // within the hospital/clinic that issued it (organizationId, mrn), not
    // globally — two different organizations legitimately reuse the same
    // MRN scheme (e.g. both starting patient numbering at "MRN-000001").
    // Before this fix (migrations/Migration00000000000000.ts's
    // patient_organization_id_mrn_unique constraint), the second insert
    // below would fail outright on a single-column unique violation on
    // "mrn" alone, regardless of organizationId.
    it('allows two different organizations to reuse the same MRN', async () => {
      const em = forkPostgresEm(setup);
      const sharedMrn = 'E2E-SHARED-MRN-001';
      const otherOrgId = '77777777-7777-7777-7777-777777777777';

      await expect(
        seedEncounter(em, {
          mrn: sharedMrn,
          icd10Code: 'Z00.00',
          procedureCode: 'PROC-001'
        })
      ).resolves.toEqual(expect.any(String));

      await expect(
        seedEncounter(em, {
          mrn: sharedMrn,
          icd10Code: 'Z00.00',
          procedureCode: 'PROC-001',
          organizationId: otherOrgId
        })
      ).resolves.toEqual(expect.any(String));
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

  describe('compliance erase/export', () => {
    const patientOf = async (encounterId: string): Promise<string> => {
      const rows = await forkPostgresEm(setup)
        .getConnection()
        .execute<{ patient_id: string }[]>(
          'select patient_id from encounter where id = ?',
          [encounterId]
        );
      return rows[0].patient_id;
    };

    afterEach(() => setTestRoles([]));

    it("exports a patient's records, then erases them", async () => {
      const encounterId = await seedEncounter(forkPostgresEm(setup), {
        mrn: 'E2E-COMPLIANCE-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
      const patientId = await patientOf(encounterId);
      setTestRoles(['system']);

      const exported = await call(baseUrl, `/compliance/export/${patientId}`, {
        token: jwt
      });
      expect(exported.status).toBe(200);
      const entities = (exported.body as { entities: Record<string, unknown[]> })
        .entities;
      expect(entities.Patient).toHaveLength(1);
      // PHI comes back decrypted under the patient's organization
      expect(entities.Patient[0]).toMatchObject({ firstName: 'Test', lastName: 'Patient' });

      const erased = await call(baseUrl, `/compliance/erase/${patientId}`, {
        method: 'DELETE',
        token: jwt
      });
      expect(erased.status).toBe(200);
      expect((erased.body as { recordsDeleted: number }).recordsDeleted).toBeGreaterThan(0);

      const rows = await forkPostgresEm(setup)
        .getConnection()
        .execute<{ n: string }[]>('select count(*) as n from patient where id = ?', [patientId]);
      expect(Number(rows[0].n)).toBe(0);
    });

    it('answers 404 for an unknown patient and 400 for a malformed id', async () => {
      setTestRoles(['system']);
      const unknown = await call(
        baseUrl,
        '/compliance/export/99999999-9999-9999-9999-999999999999',
        { token: jwt }
      );
      expect(unknown.status).toBe(404);

      const malformed = await call(baseUrl, '/compliance/export/not-a-uuid', {
        token: jwt
      });
      expect(malformed.status).toBe(400);
    });

    it('refuses callers without the system role', async () => {
      const encounterId = await seedEncounter(forkPostgresEm(setup), {
        mrn: 'E2E-COMPLIANCE-002',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
      const patientId = await patientOf(encounterId);
      const res = await call(baseUrl, `/compliance/erase/${patientId}`, {
        method: 'DELETE',
        token: jwt
      });
      expect(res.status).not.toBe(200);
      const rows = await forkPostgresEm(setup)
        .getConnection()
        .execute<{ n: string }[]>('select count(*) as n from patient where id = ?', [patientId]);
      expect(Number(rows[0].n)).toBe(1);
    });
  });

  describe('code validation', () => {
    const validate = async (codeSet: 'icd10' | 'hcpcs', rawCode: string) => {
      const routerPath = `/${codeSet}/${encodeURIComponent(rawCode)}`;
      return call(baseUrl, `/codeValidation${routerPath}`, {
        authorization: await signHmac('GET', routerPath)
      });
    };

    it('finds a code however it is written: with or without the dot, any case, padded', async () => {
      // seeded from the CDC file layout, which has no dot: E119
      for (const written of ['E11.9', 'E119', 'e11.9', ' E119']) {
        const res = await validate('icd10', written);
        expect([written, res.status]).toEqual([written, 200]);
        expect([written, (res.body as { valid: boolean }).valid]).toEqual([written, true]);
      }
      const hcpcs = await validate('hcpcs', 'j3490');
      expect((hcpcs.body as { valid: boolean }).valid).toBe(true);
    });

    it('rejects a code that is not in the release', async () => {
      const res = await validate('icd10', 'Z99.999');
      expect(res.status).toBe(200);
      expect((res.body as { valid: boolean }).valid).toBe(false);
    });
  });

  describe('scrubbing against the reference tables', () => {
    it('accepts a valid HCPCS code that the mock code set does not know', async () => {
      const encounterId = await seedEncounter(forkPostgresEm(setup), {
        mrn: 'E2E-HCPCS-001',
        icd10Code: 'J06.9',
        procedureCode: 'J3490'
      });
      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const scrubbed = await call(baseUrl, `/claim/${(built.body as { id: string }).id}/scrub`, {
        method: 'POST',
        token: jwt
      });
      expect(scrubbed.status).toBe(200);
      expect(scrubbed.body).toEqual({ status: 'ready', denials: [] });
    });

    it('a claim scrubbed ready on mock codes stays ready after a CPT license is activated', async () => {
      const em = forkPostgresEm(setup);
      const encounterId = await seedEncounter(em, {
        mrn: 'E2E-NO-RECODE-001',
        icd10Code: 'J06.9',
        procedureCode: 'PROC-001'
      });
      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const first = await call(baseUrl, `/claim/${claimId}/scrub`, { method: 'POST', token: jwt });
      expect(first.body).toEqual({ status: 'ready', denials: [] });

      await activateCptLicense(em);

      const again = await call(baseUrl, `/claim/${claimId}/scrub`, { method: 'POST', token: jwt });
      expect(again.body).toEqual({ status: 'ready', denials: [] });
    });

    it('re-scrubbing after the coder fixes the diagnosis clears the denial', async () => {
      const encounterId = await seedEncounter(forkPostgresEm(setup), {
        mrn: 'E2E-FIX-RESCRUB-001',
        icd10Code: 'J06.99', // not a real code
        procedureCode: 'PROC-001'
      });
      const built = await call(baseUrl, '/claim/build', {
        method: 'POST',
        body: { encounterId },
        token: jwt
      });
      const claimId = (built.body as { id: string }).id;
      const denied = await call(baseUrl, `/claim/${claimId}/scrub`, { method: 'POST', token: jwt });
      expect((denied.body as { status: string }).status).toBe('denied');

      await forkPostgresEm(setup)
        .getConnection()
        .execute('update diagnosis set icd10_code = ? where encounter_id = ?', ['J06.9', encounterId]);

      const fixed = await call(baseUrl, `/claim/${claimId}/scrub`, { method: 'POST', token: jwt });
      expect(fixed.body).toEqual({ status: 'ready', denials: [] });
      const open = await call(baseUrl, `/denial?claimId=${claimId}&worklistStatus=open`, { token: jwt });
      expect(open.body).toEqual([]);
    });
  });

  describe('malformed ids', () => {
    it('answers 400, not a 500 page', async () => {
      const responses = await Promise.all([
        call(baseUrl, '/denial/not-a-uuid', { token: jwt }),
        call(baseUrl, '/claim/not-a-uuid/scrub', { method: 'POST', token: jwt }),
        call(baseUrl, '/claim/build', { method: 'POST', body: { encounterId: 'abc' }, token: jwt }),
        call(baseUrl, '/denial?claimId=xyz', { token: jwt })
      ]);
      expect(responses.map((r) => r.status)).toEqual([400, 400, 400, 400]);
    });
  });

  describe('schema', () => {
    it('the migrations match the entities, so a scaffolded app gets the same constraints', async () => {
      const diff = await setup.orm!.schema.getUpdateSchemaSQL({ wrap: false });
      expect(diff.trim()).toBe('');
    });
  });

  describe('code-set loading', () => {
    const ORG = '77777777-7777-7777-7777-777777777777';
    const loadCpt = async (lines: string[], replaceRelease = false) => {
      const { Readable } = await import('node:stream');
      const { OpenTelemetryCollector } = await import('@forklaunch/core/http');
      const { CodeSetLoaderService } = await import(
        '../../persistence/etl/codeSetLoader.service'
      );
      const { loadCptCodes } = await import('../../persistence/etl/cpt.loader');
      const loader = new CodeSetLoaderService(
        forkPostgresEm(setup),
        new OpenTelemetryCollector('test', 'info', {})
      );
      return loadCptCodes(
        loader,
        Readable.from([lines.join('\n')]),
        { code: 0, description: 1, hasHeader: false },
        ORG,
        { replaceRelease }
      );
    };
    const cptCodes = async () =>
      (
        await forkPostgresEm(setup)
          .getConnection()
          .execute<{ code: string; description: string }[]>(
            'select code, description from cpt_code where organization_id = ? order by code',
            [ORG]
          )
      ).map((r) => `${r.code}=${r.description}`);

    it('loads a code repeated within one batch once, the later row winning', async () => {
      const result = await loadCpt(['99213,Office visit', '99214,Longer visit', '99213,"Office visit, established"']);
      expect(result.rowsUpserted).toBe(2);
      expect(await cptCodes()).toEqual(['99213=Office visit, established', '99214=Longer visit']);
    });

    it('counts lines it cannot use instead of skipping them silently', async () => {
      const result = await loadCpt(['99213,Office visit', 'no description here']);
      expect(result.rowsSkipped).toBe(1);
    });

    it('removes codes a full release no longer contains, and refuses a truncated release', async () => {
      await loadCpt(['99213,a', '99214,b', '99215,c', '99216,d']);
      const result = await loadCpt(['99213,a', '99214,b', '99215,c'], true);
      expect(result.rowsRetired).toBe(1);
      expect(await cptCodes()).toEqual(['99213=a', '99214=b', '99215=c']);

      await expect(loadCpt(['99213,a'], true)).rejects.toThrow(/truncated/);
      expect(await cptCodes()).toHaveLength(3);
    });
  });
});
