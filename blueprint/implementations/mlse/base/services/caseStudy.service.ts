import { queryTerms } from './ranking.service';

export type CaseSection = {
  path: string;
  text: string;
};

export type CaseStudyFields = {
  presentation?: string;
  diagnosis?: string;
  management?: string;
  outcome?: string;
};

// Section headings that case reports use (CARE guideline structure), mapped
// onto the four parts MLSE shows. Checked in order; the first match wins.
const FIELD_HEADINGS: [field: keyof CaseStudyFields, words: string[]][] = [
  ['outcome', ['outcome', 'follow', 'result', 'conclusion']],
  ['management', ['management', 'treatment', 'intervention', 'operative', 'surgery', 'procedure', 'therapy']],
  ['diagnosis', ['diagnos', 'investigation', 'imaging', 'finding', 'workup', 'work-up']],
  ['presentation', ['presentation', 'case', 'history', 'patient', 'background', 'introduction']]
];

const MAX_FIELD_CHARS = 600;

/**
 * Splits a case report into presentation, diagnosis, management and outcome
 * using its own section headings. Text is only ever copied from the report,
 * never written; parts the report does not label stay empty rather than
 * guessed.
 */
export function extractCaseStudyFields(sections: CaseSection[]): CaseStudyFields {
  const fields: CaseStudyFields = {};
  for (const section of sections) {
    const heading = section.path.toLowerCase();
    const match = FIELD_HEADINGS.find(([, words]) => words.some((w) => heading.includes(w)));
    if (!match) {
      continue;
    }
    const [field] = match;
    const existing = fields[field];
    const combined = existing ? `${existing} ${section.text}` : section.text;
    fields[field] =
      combined.length > MAX_FIELD_CHARS
        ? `${combined.slice(0, combined.lastIndexOf(' ', MAX_FIELD_CHARS)).trimEnd()}…`
        : combined;
  }
  return fields;
}

export type CaseRelevance = {
  relevant: boolean;
  reason: 'mesh' | 'terms' | 'none';
};

/**
 * Whether a case report belongs to a topic: it is tagged with the topic's
 * MeSH descriptor, or its title contains every word of one of the topic's
 * terms. A mention in the body alone does not count: case reports mention
 * past procedures in passing ("the mother had a cholecystectomy"), and
 * search engines return loosely related records, so each case is checked
 * before it is shown.
 */
export function caseRelevance(
  topic: { meshDescriptorUi?: string | null; searchTerms: string[] },
  caseReport: { meshDescriptorUis: string[]; title: string; text: string }
): CaseRelevance {
  if (topic.meshDescriptorUi && caseReport.meshDescriptorUis.includes(topic.meshDescriptorUi)) {
    return { relevant: true, reason: 'mesh' };
  }
  const words = new Set(queryTerms(caseReport.title));
  const matches = topic.searchTerms.some((term) => {
    const needed = queryTerms(term);
    return needed.length > 0 && needed.every((w) => words.has(w));
  });
  return matches ? { relevant: true, reason: 'terms' } : { relevant: false, reason: 'none' };
}

// NLM "check tags" and other descriptors that describe who was studied
// rather than what they had; they never name a diagnosis.
const NON_DIAGNOSIS_TERMS = new Set(
  [
    'humans', 'female', 'male', 'animals', 'aged', 'aged, 80 and over',
    'middle aged', 'adult', 'young adult', 'adolescent', 'child',
    'child, preschool', 'infant', 'infant, newborn', 'pregnancy',
    'diagnosis, differential', 'treatment outcome', 'retrospective studies',
    'prospective studies', 'follow-up studies', 'case reports'
  ]
);

/**
 * The heading a case study is grouped under: its disease descriptors. When
 * MeSH tree numbers are known (the full descriptor file is loaded), only
 * descriptors in the Diseases tree (C) count; otherwise every descriptor
 * except demographic check tags does. At most two are shown, so a heading
 * stays readable.
 */
export function diagnosisGroup(
  concepts: { preferredTerm: string; treeNumbers: string[] }[]
): string {
  const candidates = concepts.filter(
    (c) => !NON_DIAGNOSIS_TERMS.has(c.preferredTerm.toLowerCase())
  );
  const withTrees = candidates.filter((c) => c.treeNumbers.length > 0);
  const diseases =
    withTrees.length > 0
      ? withTrees.filter((c) => c.treeNumbers.some((t) => t.startsWith('C')))
      : candidates;
  const names = [...new Set(diseases.map((c) => c.preferredTerm))].sort();
  return names.length > 0 ? names.slice(0, 2).join('; ') : 'Unclassified';
}
