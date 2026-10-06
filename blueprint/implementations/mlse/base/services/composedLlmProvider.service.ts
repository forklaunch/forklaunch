import { LlmProvider } from '@forklaunch/interfaces-mlse/interfaces';
import {
  EmbedRequestDto,
  EmbedResponseDto,
  GenerateRequestDto,
  GenerateResponseDto,
  LlmProviderDescriptorDto
} from '@forklaunch/interfaces-mlse/types';
import { LlmProviderBase } from './llmProviderBase.service';

// Answer drafting from one provider and embeddings from another, e.g. local
// Ollama answers with development embeddings, as chosen by LLM_PROVIDER and
// EMBEDDING_PROVIDER.
export class ComposedLlmProvider extends LlmProviderBase {
  constructor(
    private readonly drafting: Pick<LlmProvider, 'generate' | 'describe'>,
    private readonly embeddings: Pick<LlmProvider, 'embed' | 'describe'>
  ) {
    super();
  }

  override generate(request: GenerateRequestDto): Promise<GenerateResponseDto> {
    return this.drafting.generate(request);
  }

  override embed(request: EmbedRequestDto): Promise<EmbedResponseDto> {
    return this.embeddings.embed(request);
  }

  override describe(): LlmProviderDescriptorDto {
    const drafting = this.drafting.describe();
    const embeddings = this.embeddings.describe();
    return {
      provider: drafting.provider,
      model: drafting.model,
      embeddingModel: embeddings.embeddingModel,
      embeddingDimensions: embeddings.embeddingDimensions
    };
  }
}
