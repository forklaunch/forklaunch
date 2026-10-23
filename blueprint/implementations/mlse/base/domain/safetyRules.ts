// The phrases that decide whether a query reaches the AI at all. Kept apart
// from the classifier's code so a clinician can review them as one list, a
// version can be recorded with each review, and a deployment can add its
// own phrases (never remove these). Phrases are matched as whole words of
// the normalized query (lowercase, anything but letters, digits and dots as
// spaces), so "I'm" is matched as "i m".
//
// STATUS: DRAFT. To be reviewed by a clinician and by legal before launch;
// record the review in `status`, `reviewedBy` and `reviewedOn`.

export type SafetyRuleLists = {
  // An emergency needs one of these near a sign it is happening now.
  acuteEvents: string[];
  // Phrases, not single words: "now", "help" or "urgent" alone also appear
  // in literature questions ("does aspirin help prevent stroke").
  happeningNow: string[];
  // An emergency as soon as it is told about one person, even in the past
  // tense ("my 3-year-old swallowed my pills"). Chronic conditions are not
  // here: "my patient had a stroke last year" is a literature question.
  acuteForAPerson: string[];
  // Someone the asker is with.
  someone: string[];
  // An emergency on its own: the asker may harm themselves.
  selfHarm: string[];
  // Taking something, which is an emergency when followed by an amount ("I
  // took 30 paracetamol") or by a poison ("my daughter drank bleach").
  ingestionVerbs: string[];
  poisons: string[];
  prescriptionRequests: string[];
  // Signs the query is about one individual rather than the literature.
  individualPatient: string[];
};

export type SafetyRules = SafetyRuleLists & {
  version: string;
  status: 'draft' | 'clinician_reviewed';
  reviewedBy?: string;
  reviewedOn?: string;
};

const FAMILY = [
  'my son',
  'my daughter',
  'my child',
  'my kid',
  'my baby',
  'my toddler',
  'my husband',
  'my wife',
  'my partner',
  'my mother',
  'my father',
  'my mom',
  'my dad',
  'my brother',
  'my sister',
  'my grandmother',
  'my grandfather'
];

export const DEFAULT_SAFETY_RULES: SafetyRules = {
  version: '2026-10-08',
  status: 'draft',
  acuteEvents: [
    'overdose',
    'overdosed',
    'cardiac arrest',
    'heart attack',
    'not breathing',
    'stopped breathing',
    'can t breathe',
    'cannot breathe',
    'not responding',
    'unresponsive',
    'collapsed',
    'seizing',
    'seizure',
    'choking',
    'anaphylaxis',
    'bleeding',
    'hemorrhaging',
    'haemorrhaging',
    'suicidal',
    'suicide',
    'poisoned',
    'swallowed',
    'chest pain',
    'stroke'
  ],
  happeningNow: [
    'right now',
    'happening now',
    'just took',
    'just swallowed',
    'just ingested',
    'just collapsed',
    'is having',
    'am having',
    'i m having',
    // "is not" alone is too broad: "bleeding risk when aspirin is not
    // stopped" is a literature question
    'is not breathing',
    'is not responding',
    'is not responsive',
    'is bleeding',
    'is unresponsive',
    'is seizing',
    'is choking',
    'has collapsed',
    'help me',
    'please help',
    'need help',
    'what do i do',
    'asap'
  ],
  acuteForAPerson: [
    'swallowed',
    'ingested',
    'overdose',
    'overdosed',
    'poisoned',
    'took too many',
    'took too much',
    'not breathing',
    'stopped breathing',
    'not responding',
    'unresponsive',
    'collapsed',
    'choking',
    'suicidal'
  ],
  someone: [...FAMILY, 'my friend', 'someone', 'somebody'],
  selfHarm: [
    'kill myself',
    'killing myself',
    'end my life',
    'i want to die',
    'i wanna die',
    'i am suicidal',
    'i m suicidal',
    'i feel suicidal',
    'hurt myself',
    'harm myself'
  ],
  ingestionVerbs: ['took', 'swallowed', 'ate', 'ingested', 'drank'],
  poisons: [
    'bleach',
    'antifreeze',
    'detergent',
    'detergent pod',
    'laundry pod',
    'battery',
    'button battery',
    'pesticide',
    'rat poison',
    'weed killer',
    'drain cleaner',
    'oven cleaner',
    'lighter fluid',
    'paint thinner',
    'methanol',
    'mushrooms'
  ],
  prescriptionRequests: [
    'prescribe me',
    'write a prescription',
    'write me a prescription',
    'prescription for me',
    'get a prescription',
    'need a prescription',
    'refill my',
    'can you prescribe',
    'please prescribe'
  ],
  individualPatient: [
    'my patient',
    'this patient',
    'our patient',
    'the patient is',
    'patient weighs',
    'he weighs',
    'she weighs',
    // clinicians' shorthand: "pt is 67M w/ CKD"
    'pt is',
    'pt has',
    'pt on',
    'pt with',
    'my pt',
    'this pt',
    'the pt',
    'should i give',
    'should i prescribe',
    'should i start',
    'what should i give',
    'how much should i give',
    'how much do i give',
    'dose for him',
    'dose for her',
    // a family member: "my mother has dementia, which medicine is best"
    ...FAMILY,
    // the asker's own treatment: "should I stop my warfarin?"
    'should i stop',
    'should i take',
    'should i continue',
    'should i skip',
    'should i double',
    'should i increase',
    'should i reduce',
    'should i lower',
    'should i switch',
    'can i stop',
    'can i take',
    'can i skip',
    'how much should i take',
    'i took',
    'i m taking',
    'i am taking',
    'i have been taking',
    'is it safe for me',
    'my dose',
    'my medication',
    'my medications',
    'my medicine',
    'my medicines',
    'my pills',
    'my tablets',
    'my blood sugar',
    'my blood pressure is'
  ]
};

const LIST_KEYS: (keyof SafetyRuleLists)[] = [
  'acuteEvents',
  'happeningNow',
  'acuteForAPerson',
  'someone',
  'selfHarm',
  'ingestionVerbs',
  'poisons',
  'prescriptionRequests',
  'individualPatient'
];

// the same normalization the classifier applies to queries
export function normalizePhrase(phrase: string): string {
  return phrase
    .toLowerCase()
    .replace(/[^a-z0-9.]+/g, ' ')
    .trim();
}

export type SafetyRuleAdditions = Partial<SafetyRuleLists> & {
  version?: string;
};

/**
 * Reads a deployment's own additions (a JSON object with any of the list
 * names, each an array of phrases). Anything else is refused, so a typo in a
 * list name cannot silently drop a rule. Keys starting with "_" are comments.
 */
export function parseSafetyRuleAdditions(value: unknown): SafetyRuleAdditions {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('Safety rule additions must be a JSON object');
  }
  const additions: SafetyRuleAdditions = {};
  for (const [key, list] of Object.entries(value)) {
    if (key === 'version') {
      if (typeof list !== 'string') {
        throw new Error('Safety rule additions: "version" must be a string');
      }
      additions.version = list;
      continue;
    }
    if (key.startsWith('_')) continue;
    if (!LIST_KEYS.includes(key as keyof SafetyRuleLists)) {
      throw new Error(
        `Safety rule additions: unknown list "${key}" (expected one of ${LIST_KEYS.join(', ')})`
      );
    }
    if (
      !Array.isArray(list) ||
      !list.every((p) => typeof p === 'string' && normalizePhrase(p).length > 0)
    ) {
      throw new Error(
        `Safety rule additions: "${key}" must be an array of non-empty phrases`
      );
    }
    additions[key as keyof SafetyRuleLists] = list as string[];
  }
  return additions;
}

/**
 * The built-in rules plus a deployment's additions. Additions only add:
 * every list here makes MLSE more careful, so a deployment can make it more
 * careful, never less.
 */
export function extendSafetyRules(
  base: SafetyRules,
  additions: SafetyRuleAdditions
): SafetyRules {
  const extended: SafetyRules = { ...base };
  for (const key of LIST_KEYS) {
    const extra = (additions[key] ?? []).map(normalizePhrase);
    extended[key] = [...new Set([...base[key], ...extra])];
  }
  if (additions.version)
    extended.version = `${base.version}+${additions.version}`;
  return extended;
}

let active: SafetyRules = DEFAULT_SAFETY_RULES;

/** The rules every classification uses unless given others. */
export function activeSafetyRules(): SafetyRules {
  return active;
}

/** Applies a deployment's additions process-wide; called once at startup. */
export function useSafetyRuleAdditions(
  additions: SafetyRuleAdditions
): SafetyRules {
  active = extendSafetyRules(DEFAULT_SAFETY_RULES, additions);
  return active;
}
