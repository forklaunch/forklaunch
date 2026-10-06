import { FetchLike, RateLimitedClient } from '../domain/http';
import { EutilsClient, EutilsOptions } from './fetchers/eutils';
import { medlinePlusClient, medlinePlusSpelling } from './fetchers/medlinePlusFetcher.service';
import { knownTerms } from './layTerms.service';
import { classifyQuery } from './queryClassifier.service';

// Endings that make a different word, not a typo: "hypertensive" is not a
// misspelling of hypertension, nor "fractures" of fracture.
const INFLECTIONS = ['s', 'es', 'ed', 'd', 'ing', 'ive', 'ic', 'al', 'ly', 'ous', 'ism', 'ia', 'ion', 'y'];

const CLINICAL_TABLES = [
  { kind: 'condition', path: 'conditions/v3/search' },
  { kind: 'procedure', path: 'procedures/v3/search' },
  { kind: 'medicine', path: 'rxterms/v3/search' }
] as const;

// The Clinical Table Search Service answers [total, codes, extra, display rows].
type ClinicalTablesResponse = [number, string[], unknown, string[][]];

function wordsOf(text: string): string[] {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim().split(' ').filter(Boolean);
}

// fewer edits for short words, where one letter turns a typo into another word
function allowedEdits(length: number): number {
  return length < 5 ? 0 : length < 10 ? 1 : 2;
}

/**
 * Edit distance with adjacent swaps ("atatck" is one edit from "attack"),
 * stopping early once it exceeds `max`.
 */
export function editDistance(a: string, b: string, max: number): number {
  if (Math.abs(a.length - b.length) > max) return max + 1;
  let before: number[] = [];
  let previous = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    let rowMin = i;
    for (let j = 1; j <= b.length; j++) {
      let value = Math.min(previous[j] + 1, row[j - 1] + 1, previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        value = Math.min(value, before[j - 2] + 1);
      }
      row.push(value);
      rowMin = Math.min(rowMin, value);
    }
    if (rowMin > max) return max + 1;
    before = previous;
    previous = row;
  }
  return previous[b.length];
}

function isInflection(word: string, term: string): boolean {
  return (
    word.startsWith(term) ||
    term.startsWith(word) ||
    INFLECTIONS.some((s) => word.endsWith(s) && word.length - s.length >= 4 && term.startsWith(word.slice(0, -s.length)))
  );
}

/**
 * Corrects known terms misspelled in a query ("heart atack symptoms" ->
 * "heart attack symptoms"), including words run together ("heartatack").
 * Conservative: the first letter must match, short words are never changed,
 * and a word that is another form of a term is left alone. Undefined when
 * nothing changes.
 */
export function localCorrection(query: string, terms: string[] = knownTerms()): string | undefined {
  const words = wordsOf(query);
  if (words.length === 0 || words.length > 12) return undefined;
  const known = terms
    .map((term) => {
      const termWords = wordsOf(term);
      return { size: termWords.length, phrase: termWords.join(' '), joined: termWords.join('') };
    })
    .filter((k) => k.size > 0)
    .sort((a, b) => b.size - a.size || b.phrase.length - a.phrase.length);

  const out: string[] = [];
  let changed = false;
  for (let i = 0; i < words.length; ) {
    const windowOf = (size: number) => (i + size <= words.length ? words.slice(i, i + size).join(' ') : undefined);
    // a term spelled correctly is kept; run together, it is spaced
    const exact = known.find((k) => windowOf(k.size) === k.phrase || words[i] === k.joined);
    if (exact) {
      const spaced = windowOf(exact.size) === exact.phrase;
      out.push(exact.phrase);
      i += spaced ? exact.size : 1;
      changed = changed || (!spaced && exact.size > 1);
      continue;
    }
    let best: { phrase: string; size: number; distance: number } | undefined;
    for (const k of known) {
      const tries: [string | undefined, number, string][] = [
        [windowOf(k.size), k.size, k.phrase],
        [words[i], 1, k.joined]
      ];
      for (const [text, size, target] of tries) {
        if (!text || text[0] !== target[0] || isInflection(text, target)) continue;
        const max = allowedEdits(target.length);
        if (max === 0) continue;
        const distance = editDistance(text, target, max);
        if (distance <= max && (!best || distance < best.distance)) {
          best = { phrase: k.phrase, size, distance };
        }
      }
    }
    if (best) {
      out.push(best.phrase);
      i += best.size;
      changed = true;
    } else {
      out.push(words[i]);
      i += 1;
    }
  }
  return changed ? out.join(' ') : undefined;
}

/**
 * True when a suggested correction only changes word forms
 * ("laparoscopic" -> "laparoscopy", "stone" -> "stones"): NCBI suggests
 * its index terms that way, but the query was spelled right.
 */
export function onlyWordForms(original: string, corrected: string): boolean {
  const a = wordsOf(original);
  const b = wordsOf(corrected);
  if (a.length !== b.length) return false;
  const changed = a.map((word, i) => [word, b[i]] as const).filter(([x, y]) => x !== y);
  return changed.length > 0 && changed.every(([x, y]) => isInflection(x, y));
}

/** Known terms that begin with what has been typed so far. */
export function localCompletions(prefix: string, terms: string[] = knownTerms()): string[] {
  const typed = wordsOf(prefix).join(' ');
  if (typed.length < 2) return [];
  const names = new Set(terms.map((term) => wordsOf(term).join(' ')));
  // "heart attacks" adds nothing next to "heart attack"
  return [...names].filter((name) => name.startsWith(typed) && !(name.endsWith('s') && names.has(name.slice(0, -1))));
}

// "Heart attack (myocardial infarction)" and "heart attack" are one suggestion
function baseName(name: string): string {
  return wordsOf(name.split(' (')[0]).join(' ');
}

function withTimeout<T>(promise: Promise<T>, ms: number, fallback: T): Promise<T> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(fallback), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      () => {
        clearTimeout(timer);
        resolve(fallback);
      }
    );
  });
}

export type QuerySuggestionOptions = {
  // each lookup at NLM gets this long; a slow one is skipped, not waited for
  timeoutMs?: number;
  cacheMs?: number;
  cacheEntries?: number;
  clinicalTablesUrl?: string;
  medlinePlusUrl?: string;
};

/**
 * Spelling corrections ("heart atack" -> "heart attack") and names that
 * complete a query as it is typed ("heart at" -> "Heart attack (myocardial
 * infarction)"), like a search engine's "Did you mean" and suggestions.
 *
 * Corrections: the local term list first, then MedlinePlus (everyday
 * spellings), then NCBI ESpell (clinical terms). Completions: the NLM
 * Clinical Table Search Service (conditions, procedures, medicines) and the
 * local term list. All three NLM services are free and public.
 *
 * As with live search, text leaves the service only when it reads as a
 * literature question, and never when it contains a digit (an age, a dose,
 * a record number). Otherwise only the local list is used.
 */
export class QuerySuggestionService {
  private readonly eutils: EutilsClient;
  private readonly medlinePlus: RateLimitedClient;
  private readonly clinicalTables: RateLimitedClient;
  private readonly cache = new Map<string, { at: number; value: unknown }>();
  private readonly timeoutMs: number;
  private readonly cacheMs: number;
  private readonly cacheEntries: number;
  private readonly clinicalTablesUrl: string;
  private readonly medlinePlusUrl: string | undefined;

  constructor(fetchImpl: FetchLike, ncbi: EutilsOptions, options: QuerySuggestionOptions = {}) {
    this.eutils = new EutilsClient('pubmed', fetchImpl, ncbi);
    this.medlinePlus = medlinePlusClient(fetchImpl);
    this.clinicalTables = new RateLimitedClient('clinicaltables', fetchImpl, 0);
    this.timeoutMs = options.timeoutMs ?? 1500;
    this.cacheMs = options.cacheMs ?? 60 * 60 * 1000;
    this.cacheEntries = options.cacheEntries ?? 1000;
    this.clinicalTablesUrl = options.clinicalTablesUrl ?? 'https://clinicaltables.nlm.nih.gov/api';
    this.medlinePlusUrl = options.medlinePlusUrl;
  }

  private mayLeave(text: string): boolean {
    const { queryClass } = classifyQuery(text);
    return (
      text.length <= 100 &&
      !/\d/.test(text) &&
      (queryClass === 'literature_lookup' || queryClass === 'dosage_question')
    );
  }

  private async cached<T>(key: string, compute: () => Promise<T>): Promise<T> {
    const hit = this.cache.get(key);
    if (hit && Date.now() - hit.at < this.cacheMs) return hit.value as T;
    const value = await compute();
    if (this.cache.size >= this.cacheEntries) {
      this.cache.delete(this.cache.keys().next().value as string);
    }
    this.cache.set(key, { at: Date.now(), value });
    return value;
  }

  /** A corrected query, or undefined when the query looks right. */
  async correct(query: string): Promise<string | undefined> {
    const text = query.trim();
    if (!text || text.length > 200) return undefined;
    const local = localCorrection(text);
    const typed = wordsOf(text).join(' ');
    // a known term spelled right needs no lookup
    if (!local && knownTerms().some((term) => wordsOf(term).join(' ') === typed)) return undefined;
    const candidate = local ?? text;
    if (!this.mayLeave(candidate)) return local;
    // NCBI's limit is shared with live search, so ESpell is only asked when
    // nothing else found a correction
    const remote = await this.cached(`correct:${local ? 'local' : 'raw'}:${candidate.toLowerCase()}`, async () => {
      const medlinePlus = await withTimeout(
        medlinePlusSpelling(this.medlinePlus, candidate, this.medlinePlusUrl),
        this.timeoutMs,
        undefined
      );
      if (medlinePlus || local) return medlinePlus;
      return withTimeout(this.eutils.spell(candidate), this.timeoutMs, undefined);
    });
    const corrected = remote && !onlyWordForms(candidate, remote) ? remote : local;
    return corrected && wordsOf(corrected).join(' ') !== typed ? corrected : undefined;
  }

  /** Up to `limit` names that complete what has been typed, best first. */
  async complete(prefix: string, limit = 6): Promise<string[]> {
    const text = prefix.trim();
    if (text.length < 2 || text.length > 100) return [];
    const local = localCompletions(text);
    const remote = this.mayLeave(text)
      ? await this.cached(`complete:${text.toLowerCase()}`, async () =>
          (await Promise.all(CLINICAL_TABLES.map((table) => this.lookUp(table, text, limit)))).flat()
        )
      : [];
    const typedWords = wordsOf(text);
    const typed = typedWords.join(' ');
    const seen = new Set<string>();
    const names = [...remote, ...local].filter((name) => {
      const key = baseName(name);
      if (!key || seen.has(key)) return false;
      seen.add(key);
      return classifyQuery(name).queryClass === 'literature_lookup';
    });
    // NLM also matches synonyms ("heart at" finds "Atrial myxoma"); names
    // containing every word typed are shown when there are any
    const containsTyped = (name: string) => {
      const words = wordsOf(name);
      return typedWords.every((t) => words.some((w) => w.startsWith(t)));
    };
    const matching = names.filter(containsTyped);
    return (matching.length > 0 ? matching : names)
      .map((name, order) => ({ name, order, starts: wordsOf(name).join(' ').startsWith(typed) }))
      .sort((a, b) => Number(b.starts) - Number(a.starts) || a.order - b.order)
      .slice(0, limit)
      .map((s) => s.name);
  }

  private async lookUp(table: (typeof CLINICAL_TABLES)[number], text: string, limit: number): Promise<string[]> {
    const url = `${this.clinicalTablesUrl}/${table.path}?terms=${encodeURIComponent(text)}&maxList=${limit}`;
    const response = await withTimeout(
      this.clinicalTables.getJson<ClinicalTablesResponse>(url),
      this.timeoutMs,
      undefined
    );
    const names = (Array.isArray(response?.[3]) ? response[3] : [])
      .map((row) => (Array.isArray(row) && typeof row[0] === 'string' ? row[0].trim() : ''))
      .filter(Boolean);
    // medicine names come as "metFORMIN XR (Oral Pill)"; the form is dropped
    return table.kind === 'medicine' ? [...new Set(names.map((n) => n.split(' (')[0].toLowerCase()))] : names;
  }
}
