import { LlmProviderParameters } from '../types/llm.types';

// Provider-neutral text generation and embeddings. Each client configures its
// own provider; swapping one requires the evaluation suite to pass again.
export interface LlmProvider<
  Params extends LlmProviderParameters = LlmProviderParameters
> {
  generate: (
    request: Params['GenerateRequestDto']
  ) => Promise<Params['GenerateResponseDto']>;
  embed: (request: Params['EmbedRequestDto']) => Promise<Params['EmbedResponseDto']>;
  describe: () => Params['LlmProviderDescriptorDto'];
}
