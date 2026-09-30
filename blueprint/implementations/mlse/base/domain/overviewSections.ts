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
  // words added to the topic for this section's own search ("myocardial
  // infarction symptoms clinical presentation"), so each section looks for
  // literature on its question instead of relying on the topic search alone
  focus?: string;
};

export const OVERVIEW_SECTIONS: Record<OverviewTopicType, OverviewSection[]> = {
  condition: [
    { key: 'what', label: 'What is it?', searchHints: ['defined', 'definition', 'characterized', 'refers', 'occurs', 'consists', 'described', 'called', 'happens', 'means'], namesTopic: true, focus: 'definition' },
    { key: 'causes', label: 'Causes and risk factors', searchHints: ['caused', 'risk', 'etiology', 'aetiology', 'pathophysiology', 'mechanism', 'mechanisms', 'triggered'], focus: 'risk factors causes' },
    { key: 'presentation', label: 'Symptoms and diagnosis', searchHints: ['symptom', 'symptoms', 'sign', 'signs', 'presentation', 'presented', 'presenting', 'pain', 'complaint', 'complaints', 'dyspnea', 'diagnosis', 'diagnosed', 'diagnostic', 'test', 'imaging', 'biomarker'], focus: 'symptoms clinical presentation diagnosis' },
    { key: 'treatment', label: 'Treatment', searchHints: ['treatment', 'treated', 'therapy', 'management', 'intervention', 'medication', 'drug', 'surgery', 'procedure'], focus: 'treatment management' },
    { key: 'outlook', label: 'Outlook and complications', searchHints: ['mortality', 'survival', 'prognosis', 'outcome', 'outcomes', 'complication', 'complications', 'recurrence'], focus: 'prognosis complications' }
  ],
  medication: [
    { key: 'uses', label: 'What is it used for?', searchHints: ['indicated', 'indication', 'indications', 'treatment', 'prophylaxis', 'prevention'] },
    { key: 'warnings', label: 'Warnings and contraindications', searchHints: ['contraindicated', 'contraindication', 'contraindications', 'warning', 'warnings', 'precaution', 'hypersensitivity'] },
    { key: 'adverse', label: 'Side effects', searchHints: ['adverse', 'reaction', 'reactions', 'side', 'effects', 'toxicity'] },
    { key: 'interactions', label: 'Interactions', searchHints: ['interaction', 'interactions', 'concomitant', 'coadministration', 'coadministered'] }
  ],
  procedure: [
    { key: 'what', label: 'What is it and why is it done?', searchHints: ['indication', 'indicated', 'performed', 'removal', 'procedure', 'treatment', 'standard'], namesTopic: true, focus: 'indications' },
    { key: 'how', label: 'How is it done?', searchHints: ['technique', 'performed', 'approach', 'dissection', 'incision', 'port', 'step', 'steps'], focus: 'surgical technique' },
    { key: 'risks', label: 'Risks and complications', searchHints: ['complication', 'complications', 'injury', 'risk', 'bleeding', 'leak', 'conversion', 'adverse'], focus: 'complications' },
    { key: 'recovery', label: 'Recovery and outcomes', searchHints: ['recovery', 'discharge', 'stay', 'postoperative', 'outcome', 'outcomes', 'return'], focus: 'postoperative recovery outcomes' }
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
 * The overview section a follow-up question asks about ("how is it
 * treated?" -> Treatment), by the section hint words it uses, or undefined
 * when it matches none.
 */
export function sectionForQuestion(
  sections: OverviewSection[],
  question: string
): OverviewSection | undefined {
  const words = new Set(queryTerms(question));
  let best: { section: OverviewSection; hits: number } | undefined;
  for (const section of sections) {
    const hits = new Set(section.searchHints.flatMap((h) => queryTerms(h)).filter((w) => words.has(w))).size;
    if (hits > 0 && (!best || hits > best.hits)) best = { section, hits };
  }
  return best?.section;
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
