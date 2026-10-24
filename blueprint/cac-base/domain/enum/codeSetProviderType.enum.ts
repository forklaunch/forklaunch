// Which CodeSetProvider a claim was actually built under — 'mock' or 'cpt',
// matching CodeSetDescriptorDto.codeSetType (blueprint/interfaces/cac).
// Recorded on the Claim itself at build time (§5, §12 item 11): once
// resolved, it's permanent history for that claim, independent of whatever
// the organization's CodeSetLicense says later. See
// plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §5's "historical claims are
// never retroactively recoded" rule.
export const CodeSetProviderType = {
  MOCK: 'mock',
  CPT: 'cpt'
} as const;
export type CodeSetProviderType =
  (typeof CodeSetProviderType)[keyof typeof CodeSetProviderType];
