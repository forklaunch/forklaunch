export type EvidencePassageDto = {
  // stable id the drafted text must cite
  id: string;
  text: string;
  // what the passage is, e.g. "PubMed case report: <title> - Management";
  // shown to the model so it can say "in a published case report"
  label?: string;
};

export type GenerateRequestDto = {
  instructions: string;
  prompt: string;
  // the only material the model may draw on
  evidence: EvidencePassageDto[];
  maxTokens?: number;
};

export type GenerateResponseDto = {
  text: string;
  model: string;
};

export type EmbedRequestDto = {
  texts: string[];
};

export type EmbedResponseDto = {
  embeddings: number[][];
  model: string;
  dimensions: number;
};

export type LlmProviderDescriptorDto = {
  provider: string;
  model: string;
  embeddingModel: string;
  embeddingDimensions: number;
};

export type LlmProviderParameters = {
  GenerateRequestDto: GenerateRequestDto;
  GenerateResponseDto: GenerateResponseDto;
  EmbedRequestDto: EmbedRequestDto;
  EmbedResponseDto: EmbedResponseDto;
  LlmProviderDescriptorDto: LlmProviderDescriptorDto;
};
