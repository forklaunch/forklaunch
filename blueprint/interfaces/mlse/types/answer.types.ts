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
};

export type AnswerSentenceDto = {
  text: string;
  // passage ids from `sources`
  citations: string[];
  // true when the sentence is quoted from a source rather than written by
  // the AI (label dosing ranges, fixed messages)
  quoted?: boolean;
};

export type AnswerSectionStatus =
  | 'answered'
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
};

export type AnswerStreamEventDto =
  | { type: 'start'; queryClass: QueryClass; kind: AnswerKind; message?: string }
  | { type: 'section'; section: AnswerSectionDto }
  | { type: 'done'; answer: AnswerResponseDto };
