// Maps to the three scrubbing-engine layers plus the two other places a claim
// can be denied — see plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §6.
export const DenialReasonCategory = {
  NCCI_PTP: 'ncci_ptp',
  NCCI_MUE: 'ncci_mue',
  LCD_NCD: 'lcd_ncd',
  REQUIRED_FIELDS: 'required_fields',
  ELIGIBILITY: 'eligibility'
} as const;
export type DenialReasonCategory =
  (typeof DenialReasonCategory)[keyof typeof DenialReasonCategory];
