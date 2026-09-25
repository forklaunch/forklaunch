import {
  EmbedRequestDto,
  EmbedResponseDto,
  GenerateRequestDto,
  GenerateResponseDto,
  LlmProviderDescriptorDto
} from '@forklaunch/interfaces-mlse/types';
import { LlmProviderBase } from './llmProviderBase.service';

// Deterministic stand-in for a real AI provider, for tests and for local
// development without an API key. It never invents content: generate() only
// restates the evidence it was given, each passage cited by id, and embed()
// derives a stable unit vector from the text.
export class FakeLlmProvider extends LlmProviderBase {
  private readonly embeddingDimensions: number;

  constructor(embeddingDimensions: number = 8) {
    super();
    if (!Number.isInteger(embeddingDimensions) || embeddingDimensions < 1) {
      throw new Error('embeddingDimensions must be a positive integer');
    }
    this.embeddingDimensions = embeddingDimensions;
  }

  override async generate({
    evidence
  }: GenerateRequestDto): Promise<GenerateResponseDto> {
    const text = evidence
      .map((passage) => `${passage.text} [${passage.id}]`)
      .join(' ');
    return { text, model: 'fake' };
  }

  override async embed({ texts }: EmbedRequestDto): Promise<EmbedResponseDto> {
    return {
      embeddings: texts.map((text) => this.vectorFor(text)),
      model: 'fake-embedding',
      dimensions: this.embeddingDimensions
    };
  }

  override describe(): LlmProviderDescriptorDto {
    return {
      provider: 'fake',
      model: 'fake',
      embeddingModel: 'fake-embedding',
      embeddingDimensions: this.embeddingDimensions
    };
  }

  private vectorFor(text: string): number[] {
    const vector = new Array<number>(this.embeddingDimensions).fill(0);
    for (let i = 0; i < text.length; i++) {
      vector[i % this.embeddingDimensions] += text.charCodeAt(i);
    }
    const norm = Math.sqrt(vector.reduce((sum, v) => sum + v * v, 0)) || 1;
    return vector.map((v) => v / norm);
  }
}
