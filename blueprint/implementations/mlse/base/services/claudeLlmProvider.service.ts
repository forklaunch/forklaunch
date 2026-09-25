import Anthropic from '@anthropic-ai/sdk';
import { LlmProvider } from '@forklaunch/interfaces-mlse/interfaces';
import {
  EmbedRequestDto,
  EmbedResponseDto,
  EvidencePassageDto,
  GenerateRequestDto,
  GenerateResponseDto,
  LlmProviderDescriptorDto
} from '@forklaunch/interfaces-mlse/types';
import { LlmProviderBase } from './llmProviderBase.service';

export const DEFAULT_CLAUDE_MODEL = 'claude-opus-5';

export type ClaudeEffort = 'low' | 'medium' | 'high' | 'xhigh' | 'max';

export type ClaudeLlmProviderOptions = {
  apiKey: string;
  model?: string;
  effort?: ClaudeEffort;
  // per request, in milliseconds
  timeoutMs?: number;
  // retries on 408, 409, 429, 5xx and connection errors
  maxRetries?: number;
  // Claude has no embeddings endpoint, so embeddings come from a separate
  // provider
  embeddings: Pick<LlmProvider, 'embed' | 'describe'>;
  // injected in tests
  client?: Anthropic;
};

// Claude declined the request (stop reason "refusal"). The answer service
// shows that section as unavailable rather than retrying elsewhere.
export class LlmRefusalError extends Error {
  constructor(readonly category: string | null) {
    super(`The AI provider declined the request${category ? ` (${category})` : ''}`);
    this.name = 'LlmRefusalError';
  }
}

// Passage text comes from the internet; a closing tag inside it must not end
// the passage early.
function formatEvidence(evidence: EvidencePassageDto[]): string {
  const passages = evidence.map((passage) => {
    const label = passage.label ? ` source="${passage.label.replaceAll('"', "'")}"` : '';
    const text = passage.text.replaceAll('</passage', '<\\/passage');
    return `<passage id="${passage.id}"${label}>\n${text}\n</passage>`;
  });
  return `<evidence>\n${passages.join('\n')}\n</evidence>`;
}

/**
 * Answer drafting with Claude through the Anthropic SDK.
 *
 * - The drafting rules go in the system prompt, marked for prompt caching
 *   because they are identical for every section; evidence and the question
 *   go in the user message.
 * - Requests stream and are read with finalMessage(), so long answers do
 *   not hit HTTP timeouts.
 * - Adaptive thinking; effort is configurable per deployment.
 * - Server-side fallbacks are on: if Claude declines a request, the API
 *   retries it on another Claude model within the same call, and the model
 *   that served it is recorded with the answer.
 *
 * Nothing it returns is shown unverified: the answer service checks every
 * sentence's citations and numbers against the evidence.
 */
export class ClaudeLlmProvider extends LlmProviderBase {
  private readonly client: Anthropic;
  private readonly model: string;
  private readonly effort: ClaudeEffort;
  private readonly embeddings: Pick<LlmProvider, 'embed' | 'describe'>;

  constructor(options: ClaudeLlmProviderOptions) {
    super();
    if (!options.client && !options.apiKey) {
      throw new Error('ClaudeLlmProvider needs an API key (LLM_API_KEY)');
    }
    this.client =
      options.client ??
      new Anthropic({
        apiKey: options.apiKey,
        timeout: options.timeoutMs ?? 120_000,
        maxRetries: options.maxRetries ?? 2
      });
    this.model = options.model || DEFAULT_CLAUDE_MODEL;
    this.effort = options.effort ?? 'high';
    this.embeddings = options.embeddings;
  }

  override async generate({
    instructions,
    prompt,
    evidence,
    maxTokens
  }: GenerateRequestDto): Promise<GenerateResponseDto> {
    const stream = this.client.beta.messages.stream({
      model: this.model,
      max_tokens: maxTokens ?? 64_000,
      betas: ['server-side-fallback-2026-07-01'],
      fallbacks: 'default',
      thinking: { type: 'adaptive' },
      output_config: { effort: this.effort },
      system: [
        { type: 'text', text: instructions, cache_control: { type: 'ephemeral' } }
      ],
      messages: [
        { role: 'user', content: `${formatEvidence(evidence)}\n\n${prompt}` }
      ]
    });
    const message = await stream.finalMessage();

    if (message.stop_reason === 'refusal') {
      throw new LlmRefusalError(message.stop_details?.category ?? null);
    }
    const text = message.content
      .map((block) => (block.type === 'text' ? block.text : ''))
      .join('');
    return { text, model: message.model };
  }

  override embed(request: EmbedRequestDto): Promise<EmbedResponseDto> {
    return this.embeddings.embed(request);
  }

  override describe(): LlmProviderDescriptorDto {
    const embeddings = this.embeddings.describe();
    return {
      provider: 'claude',
      model: this.model,
      embeddingModel: embeddings.embeddingModel,
      embeddingDimensions: embeddings.embeddingDimensions
    };
  }
}
