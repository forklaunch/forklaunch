import { ContentSourceProviderParameters } from '../types/contentSource.types';

export interface ContentSourceProvider<
  Params extends
    ContentSourceProviderParameters = ContentSourceProviderParameters
> {
  // lists the sources this provider serves, with their license terms
  describe: () => Params['SourceDescriptorDto'][];
}
