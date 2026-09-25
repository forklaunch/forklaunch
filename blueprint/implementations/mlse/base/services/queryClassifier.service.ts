import { QueryClassificationDto } from '@forklaunch/interfaces-mlse/types';
import { queryTerms } from './ranking.service';

// Rule-based and deterministic on purpose: whether a query reaches the AI at
// all must not depend on a model's judgment. Matching works on whole words
// of the normalized query (no backtracking regular expressions), so it runs
// in linear time on any input.

// An emergency needs both an acute event and a sign it is happening now.
// "Management of acetaminophen overdose" is a literature question for a
// doctor; "patient just took 40 tablets" is not.
const ACUTE_EVENTS = [
  'overdose',
  'overdosed',
  'cardiac arrest',
  'not breathing',
  'stopped breathing',
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
];
// Phrases, not single words: "now", "help" or "urgent" alone also appear in
// literature questions ("does aspirin help prevent stroke").
const HAPPENING_NOW = [
  'right now',
  'happening now',
  'just took',
  'just swallowed',
  'just ingested',
  'just collapsed',
  'is having',
  'is not',
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
];

const PRESCRIPTION_REQUESTS = [
  'prescribe me',
  'write a prescription',
  'write me a prescription',
  'prescription for me',
  'get a prescription',
  'refill my',
  'can you prescribe',
  'please prescribe'
];

// Signs the query is about one individual rather than the literature.
const INDIVIDUAL_PATIENT = [
  'my patient',
  'this patient',
  'our patient',
  'the patient is',
  'patient weighs',
  'he weighs',
  'she weighs',
  'should i give',
  'should i prescribe',
  'should i start',
  'what should i give',
  'how much should i give',
  'how much do i give',
  'dose for him',
  'dose for her'
];
const BODY_WEIGHT_UNITS = new Set(['kg', 'kgs', 'kilo', 'kilos', 'kilogram', 'kilograms', 'lb', 'lbs', 'pound', 'pounds']);
const AGE_UNITS = new Set(['year', 'years', 'yr', 'yrs', 'month', 'months', 'week', 'weeks']);
const INDIVIDUAL_ARTICLES = new Set(['a', 'an', 'my', 'this', 'our']);

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

function containsPhrase(words: string[], phrases: string[]): string | undefined {
  const text = ` ${words.join(' ')} `;
  return phrases.find((phrase) => text.includes(` ${phrase} `));
}

// "80 kg", "80kg", "a 54-year-old", "this 3 month old"
function describesIndividual(words: string[]): string | undefined {
  for (let i = 0; i < words.length; i++) {
    const word = words[i];
    const glued = word.match(/^(\d+(?:\.\d+)?)([a-z]+)$/);
    if (glued && BODY_WEIGHT_UNITS.has(glued[2])) {
      return 'body weight';
    }
    if (/^\d+(\.\d+)?$/.test(word)) {
      if (BODY_WEIGHT_UNITS.has(words[i + 1] ?? '')) {
        return 'body weight';
      }
      if (
        INDIVIDUAL_ARTICLES.has(words[i - 1] ?? '') &&
        AGE_UNITS.has(words[i + 1] ?? '') &&
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
  for (const match of query.matchAll(/\bPMID:?\s?(\d{5,9})\b/gi)) found.add(`PMID:${match[1]}`);
  for (const match of query.matchAll(/\bPMC(\d{4,9})\b/gi)) found.add(`PMC${match[1]}`);
  for (const match of query.matchAll(/\bNCT(\d{8})\b/gi)) found.add(`NCT${match[1]}`);
  for (const match of query.matchAll(/\b(10\.\d{4,9}\/[^\s"'<>]{1,200})/g)) found.add(match[1].replace(/[.,;)]+$/, ''));
  return [...found];
}

/**
 * Decides how a query is handled before any retrieval or generation.
 * Checked in order of harm: emergency, prescription, one patient, dose
 * without context, then ordinary literature lookup.
 */
export function classifyQuery(query: string): QueryClassificationDto {
  const words = normalize(query.slice(0, 2000));
  const references = sourceReferences(query.slice(0, 2000));
  const subjectTerms = queryTerms(words.filter((w) => !DOSING_WORDS.has(w)).join(' ')).filter(
    (term) => !/^\d/.test(term)
  );
  const result = (queryClass: QueryClassificationDto['queryClass'], reason: string): QueryClassificationDto => ({
    queryClass,
    reason,
    sourceReferences: references,
    subjectTerms
  });

  const acute = containsPhrase(words, ACUTE_EVENTS);
  const now = containsPhrase(words, HAPPENING_NOW);
  if (acute && now) {
    return result('emergency_pattern', `acute event "${acute}" happening "${now}"`);
  }

  const prescription = containsPhrase(words, PRESCRIPTION_REQUESTS);
  if (prescription) {
    return result('prescription_request', `asks for a prescription ("${prescription}")`);
  }

  const individual = containsPhrase(words, INDIVIDUAL_PATIENT) ?? describesIndividual(words);
  if (individual) {
    return result('patient_specific_treatment', `about one patient (${individual})`);
  }

  const asksDose =
    words.some((w) => w === 'dose' || w === 'doses' || w === 'dosage' || w === 'dosing') ||
    containsPhrase(words, ['how much', 'how many mg']) !== undefined;
  // "propofol dose" names only the drug; "cefazolin dose surgical
  // prophylaxis" gives an indication and is answered from the literature
  if (asksDose && subjectTerms.length <= 1) {
    return result('exact_dosage_no_context', 'dose asked without indication or population');
  }

  return result('literature_lookup', 'no boundary rule matched');
}
