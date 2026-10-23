import { QueryClassificationDto } from '@forklaunch/interfaces-mlse/types';
import { activeSafetyRules, type SafetyRules } from '../domain/safetyRules';
import { queryTerms } from './ranking.service';

// Rule-based and deterministic on purpose: whether a query reaches the AI at
// all must not depend on a model's judgment. Matching works on whole words
// of the normalized query (no backtracking regular expressions), so it runs
// in linear time on any input. The phrase lists live in
// domain/safetyRules.ts, where they can be reviewed and extended.

// How many words may separate an acute event from a sign it is happening
// now: "he is having a really bad seizure" has 3.
const NOW_WINDOW = 3;

// An event right after these is a point in time, not something happening:
// "useful after stroke", "following cardiac arrest".
const EVENT_AS_TIME = new Set(['after', 'before', 'following', 'post']);

// Words that turn an acute event into a topic: "seizure prophylaxis",
// "bleeding risk", "overdose management". Deliberately in code, not in the
// rules: these make the classifier LESS careful, and a deployment may only
// make it more careful.
const EVENT_AS_TOPIC = new Set([
  'prophylaxis',
  'prevention',
  'risk',
  'risks',
  'guideline',
  'guidelines',
  'protocol',
  'protocols',
  'score',
  'scores',
  'rate',
  'rates',
  'incidence',
  'threshold',
  'outcomes'
]);
// Not "history", "treatment" or "management": "chest pain, history of MI"
// is still chest pain, and missing an emergency costs more than a false
// alarm.

// After an ingestion verb: an amount ("took 30 tablets", "took too many").
const AMOUNT_PHRASES = [
  'too many',
  'too much',
  'a bottle',
  'a whole',
  'the whole',
  'all my',
  'all of',
  'all the',
  'a handful',
  'a lot'
];
// How many words after the verb an amount or poison may come.
const INGESTION_WINDOW = 4;
// A count of tablets taken at once that is worth treating as an overdose.
// "took 500 mg" is a dose, not a count, and is left alone.
const OVERDOSE_COUNT = 5;
const DOSE_UNITS = new Set([
  'mg',
  'mcg',
  'g',
  'gram',
  'grams',
  'ml',
  'units',
  'iu',
  'mmol'
]);

const BODY_WEIGHT_UNITS = new Set([
  'kg',
  'kgs',
  'kilo',
  'kilos',
  'kilogram',
  'kilograms',
  'lb',
  'lbs',
  'pound',
  'pounds'
]);
const AGE_UNITS = new Set([
  'year',
  'years',
  'yr',
  'yrs',
  'month',
  'months',
  'week',
  'weeks'
]);
const INDIVIDUAL_ARTICLES = new Set(['a', 'an', 'my', 'this', 'our']);
// "67m", "54f", "80yo", "3yom": an age, glued to a sex or "yo"
const GLUED_AGE_SEX = new Set(['m', 'f', 'yo', 'yom', 'yof']);

const DOSING_WORDS = new Set([
  'dose',
  'doses',
  'dosage',
  'dosages',
  'dosing',
  'much',
  'many',
  'mg',
  'mcg',
  'ml',
  'give',
  'administer',
  'amount'
]);

function normalize(query: string): string[] {
  return query
    .toLowerCase()
    .replace(/[^a-z0-9.]+/g, ' ')
    .split(' ')
    .filter((word) => word.length > 0)
    .map(trimDots)
    .filter((word) => word.length > 0);
}

// strips sentence dots ("dose." -> "dose") but keeps decimals ("2.5")
function trimDots(word: string): string {
  let start = 0;
  let end = word.length;
  while (start < end && word[start] === '.') start++;
  while (end > start && word[end - 1] === '.') end--;
  return word.slice(start, end);
}

function containsPhrase(
  words: string[],
  phrases: string[]
): string | undefined {
  const text = ` ${words.join(' ')} `;
  return phrases.find((phrase) => text.includes(` ${phrase} `));
}

type Occurrence = { phrase: string; start: number; end: number };

// Every place a phrase occurs, as word positions (end exclusive).
function occurrences(words: string[], phrases: string[]): Occurrence[] {
  const found: Occurrence[] = [];
  for (const phrase of phrases) {
    const parts = phrase.split(' ');
    for (let i = 0; i + parts.length <= words.length; i++) {
      let match = true;
      for (let j = 0; j < parts.length; j++) {
        if (words[i + j] !== parts[j]) {
          match = false;
          break;
        }
      }
      if (match) found.push({ phrase, start: i, end: i + parts.length });
    }
  }
  return found;
}

function isNumber(word: string | undefined): boolean {
  return word !== undefined && /^\d+(\.\d+)?$/.test(word);
}

// An acute event happening now: the event (not used as a topic, as in
// "seizure prophylaxis") within a few words of a sign it is happening now.
function acuteNow(words: string[], rules: SafetyRules): string | undefined {
  const events = occurrences(words, rules.acuteEvents).filter(
    (e) =>
      !EVENT_AS_TOPIC.has(words[e.end] ?? '') &&
      !EVENT_AS_TIME.has(words[e.start - 1] ?? '')
  );
  if (events.length === 0) return undefined;
  const nows = occurrences(words, rules.happeningNow);
  for (const event of events) {
    for (const now of nows) {
      const gap =
        event.start >= now.end
          ? event.start - now.end
          : now.start >= event.end
            ? now.start - event.end
            : 0;
      if (gap <= NOW_WINDOW) {
        return `acute event "${event.phrase}" happening "${now.phrase}"`;
      }
    }
  }
  return undefined;
}

// Someone (the asker, a person they are with, or one patient) taking an
// amount of something, or a poison: "I took 30 paracetamol", "my daughter
// drank bleach". "I took ibuprofen with my statin" has neither, and is a
// question about one person instead.
function ingestion(
  words: string[],
  rules: SafetyRules,
  person: string | undefined
): string | undefined {
  for (const verb of occurrences(words, rules.ingestionVerbs)) {
    const before = words[verb.start - 1];
    const byAPerson =
      before === 'i' || before === 'just' ? true : person !== undefined;
    if (!byAPerson) continue;
    const after = words.slice(verb.end, verb.end + INGESTION_WINDOW);
    const count = after.findIndex(
      (w, i) =>
        isNumber(w) &&
        Number(w) >= OVERDOSE_COUNT &&
        !DOSE_UNITS.has(after[i + 1] ?? '')
    );
    if (count >= 0) {
      return `took an amount ("${verb.phrase} ${after.join(' ')}")`;
    }
    const amount = containsPhrase(after, AMOUNT_PHRASES);
    if (amount) return `took "${amount}"`;
    const poison = containsPhrase(after, rules.poisons);
    if (poison) return `took a poison ("${poison}")`;
  }
  return undefined;
}

// "80 kg", "80kg", "a 54-year-old", "this 3 month old", "67M", "80 yo"
function describesIndividual(words: string[]): string | undefined {
  for (let i = 0; i < words.length; i++) {
    const word = words[i];
    const glued = word.match(/^(\d{1,3}(?:\.\d+)?)([a-z]+)$/);
    if (glued && BODY_WEIGHT_UNITS.has(glued[2])) {
      return 'body weight';
    }
    if (glued && GLUED_AGE_SEX.has(glued[2]) && !glued[1].includes('.')) {
      return 'age of one person';
    }
    if (isNumber(word)) {
      const next = words[i + 1] ?? '';
      if (BODY_WEIGHT_UNITS.has(next)) {
        return 'body weight';
      }
      if (next === 'yo' || (next === 'y' && words[i + 2] === 'o')) {
        return 'age of one person';
      }
      if (
        INDIVIDUAL_ARTICLES.has(words[i - 1] ?? '') &&
        AGE_UNITS.has(next) &&
        words[i + 2] === 'old'
      ) {
        return 'age of one person';
      }
    }
  }
  return undefined;
}

// Linear scans for identifiers; each pattern has a fixed prefix and a single
// bounded run, so there is nothing to backtrack over.
function sourceReferences(query: string): string[] {
  const found = new Set<string>();
  for (const match of query.matchAll(/\bPMID:?\s?(\d{5,9})\b/gi))
    found.add(`PMID:${match[1]}`);
  for (const match of query.matchAll(/\bPMC(\d{4,9})\b/gi))
    found.add(`PMC${match[1]}`);
  for (const match of query.matchAll(/\bNCT(\d{8})\b/gi))
    found.add(`NCT${match[1]}`);
  for (const match of query.matchAll(/\b(10\.\d{4,9}\/[^\s"'<>]{1,200})/g))
    found.add(match[1].replace(/[.,;)]+$/, ''));
  return [...found];
}

/**
 * Decides how a query is handled before any retrieval or generation.
 * Checked in order of harm: emergency, prescription, one patient, dose
 * without context, then ordinary literature lookup.
 */
export function classifyQuery(
  query: string,
  rules: SafetyRules = activeSafetyRules()
): QueryClassificationDto {
  const words = normalize(query.slice(0, 2000));
  const references = sourceReferences(query.slice(0, 2000));
  const subjectTerms = queryTerms(
    words.filter((w) => !DOSING_WORDS.has(w)).join(' ')
  ).filter((term) => !/^\d/.test(term));
  const result = (
    queryClass: QueryClassificationDto['queryClass'],
    reason: string
  ): QueryClassificationDto => ({
    queryClass,
    reason,
    sourceReferences: references,
    subjectTerms
  });

  const selfHarm = containsPhrase(words, rules.selfHarm);
  if (selfHarm) {
    return result('emergency_pattern', `risk of self-harm ("${selfHarm}")`);
  }
  const now = acuteNow(words, rules);
  if (now) {
    return result('emergency_pattern', now);
  }
  const individual =
    containsPhrase(words, rules.individualPatient) ??
    describesIndividual(words);
  const person = containsPhrase(words, rules.someone) ?? individual;
  const acuteForPerson = containsPhrase(words, rules.acuteForAPerson);
  if (acuteForPerson && person) {
    return result(
      'emergency_pattern',
      `acute event "${acuteForPerson}" told about one person (${person})`
    );
  }
  const took = ingestion(words, rules, person);
  if (took) {
    return result('emergency_pattern', took);
  }

  const prescription = containsPhrase(words, rules.prescriptionRequests);
  if (prescription) {
    return result(
      'prescription_request',
      `asks for a prescription ("${prescription}")`
    );
  }

  if (individual) {
    return result(
      'patient_specific_treatment',
      `about one patient (${individual})`
    );
  }

  const asksDose =
    words.some(
      (w) => w === 'dose' || w === 'doses' || w === 'dosage' || w === 'dosing'
    ) || containsPhrase(words, ['how much', 'how many mg']) !== undefined;
  // "propofol dose" names only the drug; "cefazolin dose surgical
  // prophylaxis" gives an indication. Neither is AI-written: both are
  // answered with the label's dosing section, quoted.
  if (asksDose && subjectTerms.length <= 1) {
    return result(
      'exact_dosage_no_context',
      'dose asked without indication or population'
    );
  }
  if (asksDose) {
    return result(
      'dosage_question',
      'dose asked: answered from the quoted label, not the AI'
    );
  }

  return result('literature_lookup', 'no boundary rule matched');
}
