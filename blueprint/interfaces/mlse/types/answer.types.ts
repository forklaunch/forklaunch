import { QueryClass } from './query.types';
import { CitablePassageDto } from './search.types';

export type AnswerRequestDto = {
  query: string;
  // answer every question and phase of a topic page instead of the query
  // alone
  topicSlug?: string;
  // also query live sources (default true; topic answers use the corpus)
  live?: boolean;
  // enables licensed sources the organization holds and, with userId,
  // records the search in the user's history
  organizationId?: string;
  userId?: string;
  // where the request came from, for history (default 'answer')
  channel?: 'answer' | 'voice';
  // overview: sections for a topic ("heart attack"); direct: one answer to
  // a specific question. Default: overview for short topic queries.
  mode?: 'overview' | 'direct';
  // a follow-up in a conversation: the topic it refers to ("heart attack"
  // for "how is it treated?"); follow-ups get one direct answer
  followUpOf?: string;
};

export type AnswerSentenceDto = {
  text: string;
  // passage ids from `sources`
  citations: string[];
  // true when the sentence is quoted from a source rather than written by
  // the AI (label dosing ranges, fixed messages)
  quoted?: boolean;
};

// quoted_evidence: the AI's draft did not pass verification, so the most
// relevant sentences from the sources are quoted as written instead
export type AnswerSectionStatus =
  | 'answered'
  | 'quoted_evidence'
  | 'insufficient_evidence'
  | 'generation_failed';

export type AnswerSectionDto = {
  key: string;
  label: string;
  number?: number;
  status: AnswerSectionStatus;
  sentences: AnswerSentenceDto[];
  // drafted sentences removed because a citation or number did not check out
  removed: number;
};

// answer: AI-written from evidence and verified; label_range: label text
// quoted without AI; boundary and emergency: fixed messages;
// source_not_found: the query names a source MLSE does not hold
export type AnswerKind =
  | 'answer'
  | 'label_range'
  | 'boundary'
  | 'emergency'
  | 'source_not_found';

// What was searched and how much of it was about the question.
export type AnswerResearchDto = {
  // the query plus the clinical and MeSH terms it was expanded to
  searchedFor: string[];
  // passages search returned
  found: number;
  // of those, passages actually about the question
  aboutQuestion: number;
  // passages given to the AI (at most two per document)
  used: number;
  // different documents they come from
  documents: number;
  // overview answers: what the topic was recognized as
  topicType?: 'condition' | 'medication' | 'procedure';
};

export type AnswerResponseDto = {
  answerId: string;
  query: string;
  queryClass: QueryClass;
  kind: AnswerKind;
  message?: string;
  notice: string;
  sections: AnswerSectionDto[];
  sources: CitablePassageDto[];
  model?: string;
  research?: AnswerResearchDto;
};

export type AnswerStreamEventDto =
  | { type: 'start'; queryClass: QueryClass; kind: AnswerKind; message?: string }
  // the sections that will follow, sent before drafting starts
  | { type: 'plan'; sections: { key: string; label: string }[]; topicType?: 'condition' | 'medication' | 'procedure' }
  | { type: 'section'; section: AnswerSectionDto }
  | { type: 'done'; answer: AnswerResponseDto };
