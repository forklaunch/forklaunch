import { CodeSetProviderParameters } from '../types/codeSet.service.types';

export interface CodeSetProvider<
  Params extends
    CodeSetProviderParameters = CodeSetProviderParameters
> {
  // looks up a procedure code and returns its descriptor, or undefined if this provider doesn't recognize it
  lookupProcedureCode: (
    lookupDto: Params['ProcedureCodeLookupDto']
  ) => Promise<Params['ProcedureCodeDto'] | undefined>;
  // reports which code set this provider serves and whether it's real, licensed AMA data
  describe: () => Params['CodeSetDescriptorDto'];
}
