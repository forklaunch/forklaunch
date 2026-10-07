export type ProcedureCodeLookupDto = {
  code: string;
};

export type ProcedureCodeDto = {
  code: string;
  description: string;
};

export type CodeSetDescriptorDto = {
  // 'mock' until an org's CPT license is active — see plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §5
  codeSetType: 'mock' | 'cpt';
  licensed: boolean;
};

export type CodeSetProviderParameters = {
  ProcedureCodeLookupDto: ProcedureCodeLookupDto;
  ProcedureCodeDto: ProcedureCodeDto;
  CodeSetDescriptorDto: CodeSetDescriptorDto;
};
