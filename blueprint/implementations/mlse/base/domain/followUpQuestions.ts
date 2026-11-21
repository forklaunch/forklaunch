// Suggested next questions after an answer, like the two follow-ups an
// assistant offers. They are fixed templates, never written by the AI, and a
// question is only suggested when the sources already found for the topic
// contain passages that address it, so following a suggestion leads to an
// answer rather than "no reliable source".
//
// STATUS: DRAFT. To be reviewed by a clinician with the overview sections.
import { classifyQuery } from '../services/queryClassifier.service';
import { queryTerms } from '../services/ranking.service';
import { OverviewTopicType, OVERVIEW_SECTIONS, sectionForQuestion } from './overviewSections';

export type FollowUpTemplate = {
  // the overview section it deepens, or undefined for a question beyond them
  sectionKey?: string;
  template: string;
  // a passage addresses the question if it contains one of these words
  hints: string[];
};

export const FOLLOW_UP_QUESTIONS: Record<OverviewTopicType, FollowUpTemplate[]> = {
  condition: [
    { sectionKey: 'presentation', template: 'What are the warning signs of {topic}?', hints: ['symptom', 'symptoms', 'sign', 'signs', 'pain', 'presentation'] },
    { sectionKey: 'presentation', template: 'How is {topic} diagnosed?', hints: ['diagnosis', 'diagnosed', 'diagnostic', 'test', 'tests', 'imaging', 'biomarker'] },
    { sectionKey: 'treatment', template: 'How is {topic} treated?', hints: ['treatment', 'treatments', 'therapy', 'management', 'medicines', 'procedure'] },
    { sectionKey: 'causes', template: 'What are the risk factors for {topic}?', hints: ['risk', 'factor', 'factors'] },
    { sectionKey: 'outlook', template: 'What are the complications of {topic}?', hints: ['complication', 'complications', 'mortality', 'prognosis'] },
    { template: 'How can {topic} be prevented?', hints: ['prevent', 'prevention', 'preventing', 'lifestyle'] },
    { template: 'How does {topic} differ in women?', hints: ['women', 'female', 'sex', 'gender'] }
  ],
  medication: [
    { sectionKey: 'uses', template: 'What is {topic} used for?', hints: ['indicated', 'indication', 'indications'] },
    { sectionKey: 'adverse', template: 'What are the side effects of {topic}?', hints: ['adverse', 'reaction', 'reactions', 'side'] },
    { sectionKey: 'warnings', template: 'Who should not take {topic}?', hints: ['contraindicated', 'contraindication', 'contraindications', 'warning', 'warnings'] },
    { sectionKey: 'interactions', template: 'What does {topic} interact with?', hints: ['interaction', 'interactions', 'concomitant'] },
    { template: 'Is {topic} safe in pregnancy?', hints: ['pregnancy', 'pregnant', 'lactation'] }
  ],
  procedure: [
    { sectionKey: 'what', template: 'Why is {topic} done?', hints: ['indication', 'indicated', 'indications'] },
    { sectionKey: 'how', template: 'How is {topic} performed?', hints: ['technique', 'performed', 'approach', 'steps'] },
    { sectionKey: 'risks', template: 'What are the risks of {topic}?', hints: ['complication', 'complications', 'risk', 'risks', 'injury'] },
    { sectionKey: 'recovery', template: 'How long is recovery after {topic}?', hints: ['recovery', 'discharge', 'stay', 'postoperative'] },
    { template: 'What are the alternatives to {topic}?', hints: ['alternative', 'alternatives', 'versus', 'compared', 'conservative'] }
  ]
};

export type SuggestFollowUpsInput = {
  topicType: OverviewTopicType;
  // as the doctor wrote it ("heart attack")
  topic: string;
  // passages already found that are about the topic
  passages: { title: string; sectionPath: string; text: string }[];
  // questions already asked in the conversation, including the current one
  asked: string[];
  // sections this answer already covers well (two or more sentences)
  coveredSections: string[];
  limit?: number;
};

/**
 * Up to two next questions: ones not asked yet, preferring parts of the topic
 * the conversation has not covered, and only where the sources already found
 * have passages that address them.
 */
export function suggestFollowUps(input: SuggestFollowUpsInput): string[] {
  const topic = input.topic.trim();
  if (!topic) return [];
  const askedText = new Set(input.asked.map((q) => q.trim().toLowerCase()));
  // a part of the topic already asked about is never suggested again; one
  // the answer already covers well is only a fallback
  const askedSections = new Set<string>();
  for (const question of input.asked) {
    const section = sectionForQuestion(OVERVIEW_SECTIONS[input.topicType], question);
    if (section) askedSections.add(section.key);
  }
  const covered = new Set(input.coveredSections);
  const passageWords = input.passages.map((p) => new Set(queryTerms(`${p.sectionPath} ${p.text}`)));

  const scored = FOLLOW_UP_QUESTIONS[input.topicType]
    .map((candidate, order) => {
      const question = candidate.template.replace('{topic}', topic);
      const hints = candidate.hints.flatMap((h) => queryTerms(h));
      const evidence = passageWords.filter((words) => hints.some((h) => words.has(h))).length;
      const isCovered = candidate.sectionKey !== undefined && covered.has(candidate.sectionKey);
      const askedAbout = candidate.sectionKey !== undefined && askedSections.has(candidate.sectionKey);
      return { question, evidence, isCovered, askedAbout, order };
    })
    // suggestions must stay literature questions: nothing that would route
    // to a boundary or emergency response
    .filter((c) => c.evidence > 0 && !askedText.has(c.question.toLowerCase()) && !c.askedAbout)
    .filter((c) => classifyQuery(c.question).queryClass === 'literature_lookup')
    .sort((a, b) => Number(a.isCovered) - Number(b.isCovered) || b.evidence - a.evidence || a.order - b.order);

  return scored.slice(0, input.limit ?? 2).map((c) => c.question);
}
