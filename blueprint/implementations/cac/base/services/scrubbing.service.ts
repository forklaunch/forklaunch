import {
  isMedicallyNecessary,
  MOCK_LCD_CROSSWALK
} from '../domain/mockLcdCrosswalk';
import { isPtpConflict, MOCK_NCCI_MUE_CAPS } from '../domain/mockNcciRules';

export type DenialReasonCategory =
  | 'ncci_ptp'
  | 'ncci_mue'
  | 'lcd_ncd'
  | 'required_fields';

export interface ScrubClaimLine {
  procedureCode: string;
  units: number;
}

export interface ScrubbingFinding {
  category: DenialReasonCategory;
  carcCode: string;
  message: string;
}

export interface ScrubbingResult {
  clean: boolean;
  findings: ScrubbingFinding[];
}

// Three distinct rule layers, kept separate because they check unrelated
// things against unrelated data — see plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md
// §6. Pure logic, no DB/entity dependency: the caller (cac-base's
// ClaimService) supplies plain line/diagnosis data and persists whatever
// this returns. Runs against mock codes today; the same logic runs
// unchanged once an organization's real-CPT connector is active (§5) — only
// the code data underneath changes.
export class ScrubbingService {
  scrub(
    lines: ReadonlyArray<ScrubClaimLine>,
    diagnosisCodes: ReadonlyArray<string>
  ): ScrubbingResult {
    const findings: ScrubbingFinding[] = [];

    // Required fields — structural completeness (CO-16: "claim/service lacks
    // information", always paired with a RARC naming what's missing), not
    // medical necessity. Checked first since it's a precondition the other
    // three layers don't themselves verify: NCCI PTP/MUE only look at
    // procedure codes that ARE present, and LCD/NCD only fires for a
    // procedure that has a mock crosswalk entry at all — a claim with zero
    // diagnoses and an uncrosswalked procedure would otherwise scrub clean.
    if (diagnosisCodes.length === 0) {
      findings.push({
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Claim has no diagnosis codes'
      });
    }
    lines.forEach((line, index) => {
      if (line.procedureCode.trim() === '') {
        findings.push({
          category: 'required_fields',
          carcCode: 'CO-16',
          message: `Charge line ${index + 1} is missing a procedure code`
        });
      }
      if (!Number.isInteger(line.units) || line.units < 1) {
        findings.push({
          category: 'required_fields',
          carcCode: 'CO-16',
          message: `Charge line ${index + 1} has an invalid unit count (${line.units}); must be a positive integer`
        });
      }
    });

    // NCCI PTP — procedure-to-procedure conflicts. CPT/HCPCS <-> CPT/HCPCS
    // only, never diagnosis codes.
    for (let i = 0; i < lines.length; i++) {
      for (let j = i + 1; j < lines.length; j++) {
        if (isPtpConflict(lines[i].procedureCode, lines[j].procedureCode)) {
          findings.push({
            category: 'ncci_ptp',
            carcCode: 'CO-97',
            message: `${lines[i].procedureCode} and ${lines[j].procedureCode} cannot be billed together without a justifying modifier`
          });
        }
      }
    }

    // NCCI MUE — implausible unit count for a single code on one date of
    // service.
    for (const line of lines) {
      const cap = MOCK_NCCI_MUE_CAPS[line.procedureCode];
      if (cap != null && line.units > cap) {
        findings.push({
          category: 'ncci_mue',
          carcCode: 'UNIT-CAP-EXCEEDED',
          message: `${line.procedureCode} billed at ${line.units} units, exceeds the mock unit cap of ${cap}`
        });
      }
    }

    // LCD/NCD — does at least one diagnosis on the claim justify this
    // procedure? Only checked for procedures with a mock crosswalk entry.
    for (const line of lines) {
      if (
        line.procedureCode in MOCK_LCD_CROSSWALK &&
        !isMedicallyNecessary(line.procedureCode, diagnosisCodes)
      ) {
        findings.push({
          category: 'lcd_ncd',
          carcCode: 'CO-50',
          message: `No diagnosis on this claim justifies ${line.procedureCode} as medically necessary`
        });
      }
    }

    return { clean: findings.length === 0, findings };
  }
}
