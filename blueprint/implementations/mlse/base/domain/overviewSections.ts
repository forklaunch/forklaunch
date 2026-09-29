// Sections of an overview answer ("heart attack", "cefazolin",
// "cholecystectomy"): the shape a doctor expects from a reference page. Each
// section is drafted from the passages that address it (they must contain
// one of its hint words) and verified on its own.
//
// STATUS: DRAFT. Engineering list; to be reviewed by a clinician with the
// question frameworks.
import { queryTerms } from '../services/ranking.service';

export type OverviewTopicType = 'condition' | 'medication' | 'procedure';

export type OverviewSection = {
  key: string;
  label: string;
  searchHints: string[];
  // sentences must also name the topic itself ("myocardial infarction is ...")
  namesTopic?: boolean;
};

export const OVERVIEW_SECTIONS: Record<OverviewTopicType, OverviewSection[]> = {
  condition: [
    { key: 'what', label: 'What is it?', searchHints: ['defined', 'definition', 'characterized', 'refers', 'occurs', 'consists', 'described'], namesTopic: true },
    { key: 'causes', label: 'Causes and risk factors', searchHints: ['caused', 'risk', 'etiology', 'aetiology', 'pathophysiology', 'mechanism', 'mechanisms', 'triggered'] },
    { key: 'presentation', label: 'Symptoms and diagnosis', searchHints: ['symptom', 'symptoms', 'sign', 'signs', 'presentation', 'presented', 'diagnosis', 'diagnosed', 'diagnostic', 'test', 'imaging', 'biomarker'] },
    { key: 'treatment', label: 'Treatment', searchHints: ['treatment', 'treated', 'therapy', 'management', 'intervention', 'medication', 'drug', 'surgery', 'procedure'] },
    { key: 'outlook', label: 'Outlook and complications', searchHints: ['mortality', 'survival', 'prognosis', 'outcome', 'outcomes', 'complication', 'complications', 'recurrence'] }
  ],
  medication: [
    { key: 'uses', label: 'What is it used for?', searchHints: ['indicated', 'indication', 'indications', 'treatment', 'prophylaxis', 'prevention'] },
    { key: 'warnings', label: 'Warnings and contraindications', searchHints: ['contraindicated', 'contraindication', 'contraindications', 'warning', 'warnings', 'precaution', 'hypersensitivity'] },
    { key: 'adverse', label: 'Side effects', searchHints: ['adverse', 'reaction', 'reactions', 'side', 'effects', 'toxicity'] },
    { key: 'interactions', label: 'Interactions', searchHints: ['interaction', 'interactions', 'concomitant', 'coadministration', 'coadministered'] }
  ],
  procedure: [
    { key: 'what', label: 'What is it and why is it done?', searchHints: ['indication', 'indicated', 'performed', 'removal', 'procedure', 'treatment', 'standard'], namesTopic: true },
    { key: 'how', label: 'How is it done?', searchHints: ['technique', 'performed', 'approach', 'dissection', 'incision', 'port', 'step', 'steps'] },
    { key: 'risks', label: 'Risks and complications', searchHints: ['complication', 'complications', 'injury', 'risk', 'bleeding', 'leak', 'conversion', 'adverse'] },
    { key: 'recovery', label: 'Recovery and outcomes', searchHints: ['recovery', 'discharge', 'stay', 'postoperative', 'outcome', 'outcomes', 'return'] }
  ]
};

// Word endings and words that name a procedure.
const PROCEDURE_SUFFIXES = ['ectomy', 'otomy', 'ostomy', 'plasty', 'scopy', 'pexy', 'rrhaphy'];
const PROCEDURE_WORDS = new Set(['surgery', 'transplant', 'transplantation', 'repair', 'bypass', 'biopsy', 'ablation', 'angioplasty', 'stent', 'stenting', 'section', 'resection', 'replacement', 'amputation', 'catheterization', 'dialysis', 'intubation']);

/**
 * Condition, medication or procedure. A procedure is recognized by its name;
 * a drug by a drug label among the relevant sources whose title is the
 * searched name (a label that merely mentions "myocardial infarction" does
 * not make "heart attack" a drug).
 */
export function detectTopicType(
  query: string,
  relevant: { sourceKey: string; title: string }[]
): OverviewTopicType {
  const raw = query.toLowerCase().split(/[^a-z]+/).filter((w) => w.length > 0);
  if (raw.some((w) => PROCEDURE_WORDS.has(w) || PROCEDURE_SUFFIXES.some((s) => w.endsWith(s)))) {
    return 'procedure';
  }
  const words = queryTerms(query);
  const isLabelFor = (title: string) => {
    const titleWords = new Set(queryTerms(title));
    return words.length > 0 && words.every((w) => titleWords.has(w));
  };
  if (relevant.some((p) => (p.sourceKey === 'openfda' || p.sourceKey === 'dailymed') && isLabelFor(p.title))) {
    return 'medication';
  }
  return 'condition';
}

/**
 * A short query naming a topic ("heart attack", "cefazolin") gets an
 * overview; a specific question ("bile duct injury rate after laparoscopic
 * cholecystectomy") gets one direct answer.
 */
export function isOverviewQuery(query: string, layMapped: boolean): boolean {
  const words = queryTerms(query).filter((w) => !/^\d/.test(w));
  return words.length > 0 && (words.length <= 2 || (layMapped && words.length <= 4));
}
