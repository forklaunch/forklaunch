// Question frameworks: the fixed set of questions every topic page answers,
// so completeness is decided by clinicians, not by the AI. Each item carries
// search hints: the words a passage must address to count as evidence for
// it.
//
// STATUS: DRAFT. Written by engineering from the approved product plan to
// exercise the pipeline. Every framework must be reviewed and approved by a
// clinician before any page built on it is shown to doctors.

export type FrameworkItem = {
  key: string;
  label: string;
  // a passage counts as evidence only if it contains at least one of these
  // (stemmed) words
  searchHints: string[];
  // items whose answers are numbers (doses, blood loss, duration); numbers
  // found in their evidence are extracted for review
  quantitative?: boolean;
};

export type FrameworkPhase = FrameworkItem & { number: number };

export type QuestionFramework = {
  key: string;
  topicType: 'procedure' | 'condition' | 'medication';
  version: number;
  status: 'draft' | 'approved';
  questions: FrameworkItem[];
  phases?: FrameworkPhase[];
};

export const PROCEDURE_FRAMEWORK: QuestionFramework = {
  key: 'procedure-v1',
  topicType: 'procedure',
  version: 1,
  status: 'draft',
  questions: [
    { key: 'what', label: 'What is it?', searchHints: ['removal', 'defined', 'definition', 'consists', 'involves'] },
    { key: 'why', label: 'Why is it done?', searchHints: ['indication', 'indicated', 'symptomatic', 'cholelithiasis', 'cholecystitis'] },
    { key: 'who', label: 'Who is it for, and who should not have it?', searchHints: ['selection', 'eligible', 'contraindication', 'contraindicated', 'candidate'] },
    { key: 'when', label: 'When is it done?', searchHints: ['timing', 'delayed', 'emergency', 'elective', 'interval'] },
    { key: 'how', label: 'How is it done?', searchHints: ['trocar', 'dissection', 'retraction', 'clipped', 'pneumoperitoneum'] },
    { key: 'risks', label: 'What are the risks?', searchHints: ['complication', 'injury', 'leak', 'bleeding', 'conversion'] },
    { key: 'after', label: 'What happens after?', searchHints: ['postoperative', 'discharge', 'readmission', 'analgesia', 'recovery'] },
    { key: 'outcomes', label: 'What are the outcomes?', searchHints: ['outcome', 'mortality', 'morbidity', 'success', 'efficacy'], quantitative: true }
  ],
  phases: [
    { number: 1, key: 'preop', label: 'Pre-operative preparation', searchHints: ['preoperative', 'prophylaxis', 'antibiotic', 'fasting', 'crossmatch', 'investigation'] },
    { number: 2, key: 'anesthesia', label: 'Anesthesia', searchHints: ['anesthesia', 'anaesthesia', 'induction', 'intubation', 'propofol', 'mg/kg'], quantitative: true },
    { number: 3, key: 'positioning', label: 'Positioning and preparation', searchHints: ['position', 'supine', 'trendelenburg', 'drape'] },
    { number: 4, key: 'access', label: 'Access: how it starts', searchHints: ['incision', 'port', 'trocar', 'pneumoperitoneum', 'insufflation', 'umbilical'], quantitative: true },
    { number: 5, key: 'core', label: 'Core operative steps', searchHints: ['dissection', 'critical', 'cystic', 'clip', 'divided', 'triangle'] },
    { number: 6, key: 'blood', label: 'Blood management', searchHints: ['blood', 'bleeding', 'hemorrhage', 'haemorrhage', 'transfusion'], quantitative: true },
    { number: 7, key: 'closure', label: 'Closure: how it ends', searchHints: ['closure', 'drain', 'fascia', 'suture', 'hemostasis'] },
    { number: 8, key: 'emergence', label: 'Emergence and immediate post-op', searchHints: ['extubation', 'emergence', 'recovery', 'nausea', 'analgesia'] },
    { number: 9, key: 'recovery', label: 'Post-operative course and recovery', searchHints: ['discharge', 'stay', 'recovery', 'readmission', 'follow'], quantitative: true }
  ]
};

export const CONDITION_FRAMEWORK: QuestionFramework = {
  key: 'condition-v1',
  topicType: 'condition',
  version: 1,
  status: 'draft',
  questions: [
    { key: 'what', label: 'What is it?', searchHints: ['defined', 'definition', 'disease', 'disorder'] },
    { key: 'why', label: 'Why does it happen?', searchHints: ['cause', 'etiology', 'aetiology', 'pathophysiology'] },
    { key: 'risk', label: 'Who is at risk?', searchHints: ['risk', 'factor', 'prevalence', 'incidence'], quantitative: true },
    { key: 'symptoms', label: 'How does it present?', searchHints: ['symptom', 'sign', 'present', 'pain'] },
    { key: 'diagnosis', label: 'How is it diagnosed?', searchHints: ['diagnosis', 'diagnostic', 'imaging', 'ultrasound', 'test'] },
    { key: 'treatment', label: 'How is it treated?', searchHints: ['treatment', 'management', 'therapy', 'surgery'] },
    { key: 'complications', label: 'What are the complications?', searchHints: ['complication', 'risk', 'recurrence'] },
    { key: 'prognosis', label: 'What is the outlook?', searchHints: ['prognosis', 'outcome', 'survival', 'mortality'], quantitative: true },
    { key: 'prevention', label: 'Can it be prevented?', searchHints: ['prevention', 'prevent', 'screening'] }
  ]
};

export const MEDICATION_FRAMEWORK: QuestionFramework = {
  key: 'medication-v1',
  topicType: 'medication',
  version: 1,
  status: 'draft',
  questions: [
    { key: 'indications', label: 'What is it used for?', searchHints: ['indication', 'indicated', 'prophylaxis', 'treatment'] },
    { key: 'dosing', label: 'How is it dosed (as labelled)?', searchHints: ['dose', 'dosage', 'administration', 'mg', 'mg/kg'], quantitative: true },
    { key: 'contraindications', label: 'Who should not take it?', searchHints: ['contraindication', 'contraindicated', 'hypersensitivity'] },
    { key: 'warnings', label: 'What are the warnings?', searchHints: ['warning', 'precaution', 'boxed'] },
    { key: 'interactions', label: 'What does it interact with?', searchHints: ['interaction', 'concomitant', 'coadministration'] },
    { key: 'adverse', label: 'What are the side effects?', searchHints: ['adverse', 'reaction', 'side', 'effect'] },
    { key: 'populations', label: 'Special populations', searchHints: ['pregnancy', 'pediatric', 'paediatric', 'geriatric', 'renal', 'hepatic'] },
    { key: 'monitoring', label: 'What should be monitored?', searchHints: ['monitor', 'monitoring', 'level', 'function'] }
  ]
};

export const QUESTION_FRAMEWORKS: Record<string, QuestionFramework> = {
  [PROCEDURE_FRAMEWORK.key]: PROCEDURE_FRAMEWORK,
  [CONDITION_FRAMEWORK.key]: CONDITION_FRAMEWORK,
  [MEDICATION_FRAMEWORK.key]: MEDICATION_FRAMEWORK
};

// Every question and phase of a framework, phases after questions.
export function frameworkItems(framework: QuestionFramework): (FrameworkItem & { kind: 'question' | 'phase'; number?: number })[] {
  return [
    ...framework.questions.map((q) => ({ ...q, kind: 'question' as const })),
    ...(framework.phases ?? []).map((p) => ({ ...p, kind: 'phase' as const }))
  ];
}
