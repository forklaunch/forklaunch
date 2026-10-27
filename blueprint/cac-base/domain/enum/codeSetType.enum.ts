export const CodeSetType = {
  ICD10: 'icd10',
  HCPCS: 'hcpcs',
  CPT: 'cpt'
} as const;
export type CodeSetType = (typeof CodeSetType)[keyof typeof CodeSetType];
