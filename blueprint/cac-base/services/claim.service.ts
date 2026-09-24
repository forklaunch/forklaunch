import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { ScrubbingService } from '@forklaunch/implementation-cac-base/services';
import type {
  DenialReasonCategory as MockDenialReasonCategory,
  ScrubbingFinding
} from '@forklaunch/implementation-cac-base/services';
import { EntityManager } from '@mikro-orm/postgresql';
import { ClaimStatus } from '../domain/enum/claimStatus.enum';
import { CodeSetProviderType } from '../domain/enum/codeSetProviderType.enum';
import { DenialReasonCategory } from '../domain/enum/denialReasonCategory.enum';
import { WorklistStatus } from '../domain/enum/worklistStatus.enum';
import { Claim } from '../persistence/entities/claim.entity';
import { Denial } from '../persistence/entities/denial.entity';
import { Encounter } from '../persistence/entities/encounter.entity';
import { CodeSetProviderResolver } from './codeSetProviderResolver.service';
import { CodeValidationService } from './codeValidation.service';

export interface ScrubClaimResult {
  status: ClaimStatus;
  denials: Denial[];
}

// Builds a claim from one encounter's charges + diagnoses, then runs it
// through the three-layer scrubbing engine (§6). Lives in cac-base, not
// implementations/cac/base, because it needs the real entities — unlike
// CodeSetProvider, there's no swappable mock/real variant of "how a claim
// gets built," since cac has only one variant (cac-base, §3). The scrubbing
// *logic* itself (ScrubbingService) is a pure function with no DB
// dependency, so it stays in implementations/cac/base and is reused as-is.
export class ClaimService {
  constructor(
    private readonly em: EntityManager,
    private readonly scrubbingService: ScrubbingService,
    private readonly codeSetProviderResolver: CodeSetProviderResolver,
    private readonly codeValidationService: CodeValidationService,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async buildClaim(
    organizationId: string,
    encounterId: string
  ): Promise<Claim | null> {
    // organizationId is part of the lookup, not just the write — without it
    // here, any caller who knows (or guesses) another organization's
    // encounterId could build a claim from it, tagged with that OTHER
    // organization's id (copied from the encounter), not their own. Scoping
    // the read to the caller's own org turns that into a real 404 instead.
    // findOne (not findOneOrFail) + a null return, matching the house
    // pattern (denialWorklist.service.ts) — findOneOrFail's NotFoundError
    // isn't caught anywhere in the Express error handler and renders as a
    // generic 500, not the 404 this was meant to produce.
    const encounter = await this.em.findOne(
      Encounter,
      { id: encounterId, organizationId },
      { populate: ['charges', 'diagnoses', 'patient'] }
    );
    if (!encounter) {
      return null;
    }

    // Resolved once, here, and never again for this claim (§5's "historical
    // claims are never retroactively recoded" rule) — a CodeSetLicense flip
    // to active only affects claims built *after* the flip. scrubClaim
    // reads this stored value back rather than re-resolving it.
    const codeSetProvider =
      await this.codeSetProviderResolver.resolve(organizationId);
    const codeSetType =
      codeSetProvider.describe().codeSetType === 'cpt'
        ? CodeSetProviderType.CPT
        : CodeSetProviderType.MOCK;

    const claim = this.em.create(Claim, {
      organizationId: encounter.organizationId,
      patient: encounter.patient,
      encounter,
      status: ClaimStatus.DRAFT,
      codeSetType
    });

    await this.em.persist(claim).flush();

    this.otel.info('Built claim from encounter', {
      claimId: claim.id,
      encounterId,
      codeSetType
    });

    return claim;
  }

  async scrubClaim(
    organizationId: string,
    claimId: string
  ): Promise<ScrubClaimResult | null> {
    const claim = await this.em.findOne(
      Claim,
      { id: claimId, organizationId },
      { populate: ['encounter', 'encounter.charges', 'encounter.diagnoses'] }
    );
    if (!claim) {
      return null;
    }

    const lines = claim.encounter.charges
      .getItems()
      .map((charge) => ({
        procedureCode: charge.procedureCode,
        units: charge.units
      }));
    const diagnosisCodes = claim.encounter.diagnoses
      .getItems()
      .map((diagnosis) => diagnosis.icd10Code);

    // Unknown-procedure-code check. ScrubbingService is a pure function
    // with no DB dependency (see the class comment above) and can't answer
    // "does this code actually exist" itself — none of its three original
    // layers do either: NCCI PTP/MUE only match a code against a table of
    // *known* codes (an unrecognized code just never matches, silently),
    // and LCD/NCD only fires for a procedure that already has a crosswalk
    // entry. A well-formed but mistyped/nonexistent code (e.g. a stray
    // digit) previously scrubbed clean regardless. Closing that gap needs
    // the organization's actual CodeSetProvider — an async, DB-backed
    // lookup — so it lives here rather than in the pure scrubbing engine.
    // Re-resolves rather than trusting claim.codeSetType: unlike which
    // provider *built* this claim (§5's "never retroactively recoded"
    // rule, which only governs that historical record), whether a code
    // exists is a fact about the present code set, checked fresh every
    // scrub.
    const codeSetProvider =
      await this.codeSetProviderResolver.resolve(organizationId);
    const unknownProcedureFindings: ScrubbingFinding[] = [];
    for (let i = 0; i < lines.length; i++) {
      const known = await codeSetProvider.lookupProcedureCode({
        code: lines[i].procedureCode
      });
      if (!known) {
        unknownProcedureFindings.push({
          category: 'required_fields',
          carcCode: 'CO-16',
          message: `Charge line ${i + 1} references procedure code "${lines[i].procedureCode}", which is not recognized by the active code-set provider`
        });
      }
    }

    // Unknown-diagnosis-code check. ICD-10-CM is a free, public code set —
    // unlike procedure codes, there's a real reference-table validator for
    // it already (CodeValidationService, backing the standalone
    // /codeValidation/icd10/:code endpoint) — it just was never called from
    // anywhere in the claim-building/scrubbing path until now. Same
    // reasoning as the procedure check above: LCD/NCD only asks "does this
    // diagnosis justify this procedure," never "is this a real diagnosis
    // code at all," so a mistyped/nonexistent ICD-10 code previously
    // scrubbed clean as long as it happened not to match any crosswalk
    // entry.
    const unknownDiagnosisFindings: ScrubbingFinding[] = [];
    for (let i = 0; i < diagnosisCodes.length; i++) {
      const validated = await this.codeValidationService.validateIcd10(
        diagnosisCodes[i]
      );
      if (!validated.valid) {
        unknownDiagnosisFindings.push({
          category: 'required_fields',
          carcCode: 'CO-16',
          message: `Diagnosis ${i + 1} references ICD-10-CM code "${diagnosisCodes[i]}", which is not a recognized code`
        });
      }
    }

    const result = this.scrubbingService.scrub(lines, diagnosisCodes);
    const findings = [
      ...unknownProcedureFindings,
      ...unknownDiagnosisFindings,
      ...result.findings
    ];

    // Re-scrubbing (e.g. after the charges/diagnoses change) must not just
    // append to the previous scrub's findings — the stale OPEN denials from
    // last time would sit on the worklist forever even after the issue is
    // fixed, and AnalyticsService.denialsByCategory (which counts every OPEN
    // + RESOLVED denial) would silently drift out of sync with deniedCount
    // (which only reflects the claim's *current* status). Already-RESOLVED
    // denials are left alone — they're a real worklist record of past work,
    // not a byproduct of this scrub.
    const staleOpenDenials = await this.em.find(Denial, {
      claim,
      worklistStatus: WorklistStatus.OPEN
    });
    if (staleOpenDenials.length > 0) {
      this.em.remove(staleOpenDenials);
    }

    const denials = findings.map((finding) =>
      this.em.create(Denial, {
        organizationId: claim.organizationId,
        claim,
        carcCode: finding.carcCode,
        category: mapMockCategory(finding.category),
        worklistStatus: WorklistStatus.OPEN
      })
    );

    claim.status = findings.length === 0 ? ClaimStatus.READY : ClaimStatus.DENIED;

    if (denials.length > 0) {
      this.em.persist(denials);
    }
    await this.em.persist(claim).flush();

    this.otel.info('Scrubbed claim', {
      claimId,
      status: claim.status,
      findingCount: findings.length
    });

    return { status: claim.status, denials };
  }
}

function mapMockCategory(
  category: MockDenialReasonCategory
): DenialReasonCategory {
  switch (category) {
    case 'ncci_ptp':
      return DenialReasonCategory.NCCI_PTP;
    case 'ncci_mue':
      return DenialReasonCategory.NCCI_MUE;
    case 'lcd_ncd':
      return DenialReasonCategory.LCD_NCD;
    case 'required_fields':
      return DenialReasonCategory.REQUIRED_FIELDS;
  }
}
