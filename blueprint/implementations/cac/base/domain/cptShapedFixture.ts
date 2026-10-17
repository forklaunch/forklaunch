// Synthetic, CPT-*shaped* fixture data — real CPT numeric code-range
// structure (Category I: 5-digit numeric, e.g. 10000-69990 surgery,
// 70000-79999 radiology, 90000-99999 medicine/E&M; Category II: 4 digits +
// "F"; Category III: 4 digits + "T"), but every code number and description
// below is fabricated for structural testing only. None of this is real AMA
// CPT content, and no code here is asserted to correspond to any real,
// currently-assigned CPT code — this exists purely to prove the scrubbing
// engine (§6) has no hidden assumption baked in that only works against
// MockProcedureCodeProvider's "PROC-XXX" placeholder format, before any
// adopting organization ever wires in their own real, licensed CPT feed
// (§5's readiness bar, item 4). See
// plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §5, §12 item 3.
export const CPT_SHAPED_PTP_CONFLICTS: ReadonlyArray<
  readonly [string, string]
> = [
  // Two synthetic Category I codes in the surgery range.
  ['10001', '10002']
];

export const CPT_SHAPED_MUE_CAPS: Record<string, number> = {
  // Synthetic Category I code, radiology range.
  '70001': 1,
  // Synthetic Category III (emerging technology) code.
  '0001T': 2
};

export const CPT_SHAPED_LCD_CROSSWALK: Record<string, ReadonlyArray<string>> =
  {
    // Synthetic Category I code, medicine/E&M range — justified by the same
    // real, free ICD-10-CM code used elsewhere in the mock LCD crosswalk.
    '90001': ['Z00.00']
  };
