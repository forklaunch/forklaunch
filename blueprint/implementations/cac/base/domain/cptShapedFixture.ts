// Synthetic, CPT-*shaped* fixture data — real CPT numeric code-range
// structure (Category I: 5-digit numeric; Category II: 4 digits + "F";
// Category III: 4 digits + "T"), but every code number and description
// below is fabricated for structural testing only. None of this is real
// AMA CPT content, and no code here corresponds to any real, assigned CPT
// code.
//
// This exists purely to prove the scrubbing engine (§6) has no hidden
// assumption baked in that only works against MockProcedureCodeProvider's
// "PROC-XXX" placeholder format, before any adopting organization wires in
// their own real, licensed CPT feed (§5's readiness bar, item 4). See
// plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §5, §12 item 3.
//
// Every number is deliberately picked to fall OUTSIDE the assigned
// Category I sub-ranges rather than merely to look synthetic: surgery ends
// at 69990, radiology starts at 70010, medicine starts at 90281, and
// Category III assignments sit far below 9999T. Earlier revisions used
// 10001/10002/0001T, which are structurally identical but are real codes
// (0001T is CMS-deleted), so they were replaced. Keep any new fixture code
// outside those ranges too — the engine only cares about the shape.
export const CPT_SHAPED_PTP_CONFLICTS: ReadonlyArray<
  readonly [string, string]
> = [
  // Two synthetic Category I codes just past the surgery range end (69990).
  ['69991', '69992']
];

export const CPT_SHAPED_MUE_CAPS: Record<string, number> = {
  // Synthetic Category I code just below the radiology range start (70010).
  '70001': 1,
  // Synthetic Category III code, far above any assigned 4-digit + 'T' value.
  '9999T': 2
};

export const CPT_SHAPED_LCD_CROSSWALK: Record<string, ReadonlyArray<string>> = {
  // Synthetic Category I code below the medicine/E&M start (90281),
  // justified by the same real, free ICD-10-CM code used elsewhere in
  // the mock LCD crosswalk.
  '90001': ['Z00.00']
};
