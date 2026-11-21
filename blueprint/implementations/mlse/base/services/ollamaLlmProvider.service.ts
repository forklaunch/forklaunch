import {
  EmbedRequestDto,
  EmbedResponseDto,
  GenerateRequestDto,
  GenerateResponseDto,
  LlmProviderDescriptorDto
} from '@forklaunch/interfaces-mlse/types';
import { userMessage } from '../domain/evidenceFormat';
import { LlmProviderBase } from './llmProviderBase.service';

export const DEFAULT_OLLAMA_URL = 'http://localhost:11434';
export const DEFAULT_OLLAMA_MODEL = 'qwen2.5:3b';
export const DEFAULT_OLLAMA_EMBEDDING_MODEL = 'nomic-embed-text';
// nomic-embed-text produces 768-dimensional vectors
export const DEFAULT_OLLAMA_EMBEDDING_DIMENSIONS = 768;

// Models trained with a task instruction in front of each text. Without it
// they still embed, but rank less well. Ollama does not add it.
const EMBEDDING_TASK_PREFIXES: Record<string, { query: string; document: string }> = {
  'nomic-embed-text': { query: 'search_query: ', document: 'search_document: ' }
};

// The subset of fetch the provider uses, injected so tests never need a
// running Ollama.
export type PostLike = (
  url: string,
  init: {
    method: 'POST';
    headers: Record<string, string>;
    body: string;
    signal?: AbortSignal;
  }
) => Promise<{ ok: boolean; status: number; text: () => Promise<string> }>;

export type OllamaLlmProviderOptions = {
  baseUrl?: string;
  model?: string;
  embeddingModel?: string;
  // must match the embedding model; fixes the vector column size
  embeddingDimensions?: number;
  // per request, in milliseconds (local models on a laptop are slow)
  timeoutMs?: number;
  // most tokens one answer section may use
  maxTokens?: number;
  // prompt window; must hold the rules plus the evidence (default 8192)
  contextTokens?: number;
  post?: PostLike;
};

export class OllamaRequestError extends Error {
  constructor(readonly status: number, detail: string) {
    super(`Ollama request failed with HTTP ${status}: ${detail.slice(0, 200)}`);
    this.name = 'OllamaRequestError';
  }
}

/**
 * Free answer drafting and embeddings with open-source models run locally
 * through Ollama (https://ollama.com). No API key and no per-request cost;
 * the deployment supplies the hardware.
 *
 * Small models write weaker answers than Claude. They go through the same
 * checks: a sentence whose citation, wording or numbers do not match its
 * source is removed, so a weaker model yields shorter answers, not wrong
 * ones.
 */
export class OllamaLlmProvider extends LlmProviderBase {
  private readonly baseUrl: string;
  private readonly model: string;
  private readonly embeddingModel: string;
  private readonly embeddingDimensions: number;
  private readonly timeoutMs: number;
  private readonly maxTokens: number;
  private readonly contextTokens: number;
  private readonly post: PostLike;

  constructor(options: OllamaLlmProviderOptions = {}) {
    super();
    let baseUrl = options.baseUrl || DEFAULT_OLLAMA_URL;
    while (baseUrl.endsWith('/')) baseUrl = baseUrl.slice(0, -1);
    this.baseUrl = baseUrl;
    this.model = options.model || DEFAULT_OLLAMA_MODEL;
    this.embeddingModel = options.embeddingModel || DEFAULT_OLLAMA_EMBEDDING_MODEL;
    this.embeddingDimensions = options.embeddingDimensions ?? DEFAULT_OLLAMA_EMBEDDING_DIMENSIONS;
    this.timeoutMs = options.timeoutMs ?? 180_000;
    // one section is a few sentences; the cap also stops a runaway loop
    this.maxTokens = options.maxTokens ?? 800;
    this.contextTokens = options.contextTokens ?? 8_192;
    this.post = options.post ?? ((url, init) => fetch(url, init));
    if (!Number.isInteger(this.embeddingDimensions) || this.embeddingDimensions < 1) {
      throw new Error('embeddingDimensions must be a positive integer');
    }
  }

  override async generate({
    instructions,
    prompt,
    evidence,
    maxTokens
  }: GenerateRequestDto): Promise<GenerateResponseDto> {
    const response = await this.request<{ model?: string; message?: { content?: string } }>(
      '/api/chat',
      {
        model: this.model,
        stream: false,
        messages: [
          { role: 'system', content: instructions },
          { role: 'user', content: userMessage(evidence, prompt) }
        ],
        // Deterministic and short: the task is extraction, not creativity.
        // Ollama reads only 2048 tokens by default and silently drops the
        // start of a longer prompt (the drafting rules), so the window is
        // set to fit the rules plus the evidence.
        options: {
          temperature: 0,
          num_predict: maxTokens ?? this.maxTokens,
          // small models can repeat one sentence many times
          repeat_penalty: 1.15,
          num_ctx: this.contextTokens
        }
      }
    );
    return { text: response.message?.content ?? '', model: response.model ?? this.model };
  }

  override async embed({ texts, purpose }: EmbedRequestDto): Promise<EmbedResponseDto> {
    if (texts.length === 0) {
      return { embeddings: [], model: this.embeddingModel, dimensions: this.embeddingDimensions };
    }
    // 'nomic-embed-text:v1.5' is still nomic-embed-text
    const prefixes = EMBEDDING_TASK_PREFIXES[this.embeddingModel.split(':')[0]];
    const prefix = purpose && prefixes ? prefixes[purpose] : '';
    const response = await this.request<{ embeddings?: number[][] }>('/api/embed', {
      model: this.embeddingModel,
      input: texts.map((text) => prefix + text)
    });
    const embeddings = response.embeddings ?? [];
    if (embeddings.length !== texts.length) {
      throw new Error(`Ollama returned ${embeddings.length} embeddings for ${texts.length} texts`);
    }
    // a model with another size would silently break vector search
    const wrong = embeddings.find((vector) => vector.length !== this.embeddingDimensions);
    if (wrong) {
      throw new Error(
        `${this.embeddingModel} returned ${wrong.length} dimensions; EMBEDDING_DIMENSIONS is ${this.embeddingDimensions}`
      );
    }
    return { embeddings, model: this.embeddingModel, dimensions: this.embeddingDimensions };
  }

  override describe(): LlmProviderDescriptorDto {
    return {
      provider: 'ollama',
      model: this.model,
      embeddingModel: this.embeddingModel,
      embeddingDimensions: this.embeddingDimensions
    };
  }

  private async request<T>(path: string, body: unknown): Promise<T> {
    const response = await this.post(`${this.baseUrl}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(this.timeoutMs)
    });
    const text = await response.text();
    if (!response.ok) {
      throw new OllamaRequestError(response.status, text);
    }
    return JSON.parse(text) as T;
  }
}
