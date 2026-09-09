// Free, non-AMA mock NCCI PTP/MUE data — carries the same shape as real CMS
// NCCI edit tables without using any real code list. Illustrative only; a
// coding/compliance SME should review before this fixture is treated as
// authoritative for anything beyond demoing the scrubbing engine's logic.
// See plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §6, §12 item 3.
import {
  CPT_SHAPED_MUE_CAPS,
  CPT_SHAPED_PTP_CONFLICTS
} from './cptShapedFixture';

// NCCI PTP — unordered pairs of mock procedure codes that shouldn't be
// billed together on the same claim absent a justifying modifier (which
// this mock fixture doesn't model — every pair here is a hard conflict).
// Includes both the "PROC-XXX" placeholder pairs and the CPT-*shaped*
// synthetic pairs (§5 readiness bar) — the same lookup, same function,
// proving there's no format assumption baked in.
export const MOCK_NCCI_PTP_CONFLICTS: ReadonlyArray<
  readonly [string, string]
> = [['PROC-001', 'PROC-002'], ...CPT_SHAPED_PTP_CONFLICTS];

export function isPtpConflict(codeA: string, codeB: string): boolean {
  return MOCK_NCCI_PTP_CONFLICTS.some(
    ([a, b]) =>
      (a === codeA && b === codeB) || (a === codeB && b === codeA)
  );
}

// NCCI MUE — the maximum plausible unit count for a single mock procedure
// code on one date of service. Codes not listed here have no mock unit cap.
export const MOCK_NCCI_MUE_CAPS: Record<string, number> = {
  'PROC-001': 1,
  'PROC-002': 1,
  'PROC-003': 3,
  'PROC-004': 1,
  'PROC-005': 1,
  'PROC-006': 1,
  'PROC-007': 2,
  'PROC-008': 1,
  'PROC-009': 1,
  'PROC-010': 2,
  ...CPT_SHAPED_MUE_CAPS
};
