import { verifyNumbers } from './numberVerifier.service';
import { checkSupport } from './supportCheck.service';
import { queryTerms } from './ranking.service';

export type VerifiedSentence = { text: string; citations: string[] };
export type RemovedSentence = { text: string; reason: string };
export type DraftVerification = {
  kept: VerifiedSentence[];
  removed: RemovedSentence[];
};

export type CitationVerifierOptions = {
  // share of a sentence's content words that must appear in the passages it
  // cites
  minSupport?: number;
};

const DEFAULT_MIN_SUPPORT = 0.6;
const MAX_SENTENCE_LENGTH = 1000;

// "[P1]" markers; a fixed prefix and a bounded digit run, so linear.
const MARKER = /\[(P\d{1,4})\]/g;

// A sentence ends at . ! or ? followed (after any markers and spaces) by a
// capital letter. Decimals ("2.5") and abbreviations before a lowercase word
// do not split. A plain character scan, so linear on any input.
function splitSentences(line: string): string[] {
  const sentences: string[] = [];
  let start = 0;
  let i = 0;
  while (i < line.length) {
    const ch = line[i];
    if (ch === '.' || ch === '!' || ch === '?') {
      let j = i + 1;
      while (j < line.length) {
        if (line[j] === ' ' || line[j] === '\t') {
          j++;
          continue;
        }
        const marker = line[j] === '[' ? /^\[P\d{1,4}\]/.exec(line.slice(j, j + 7)) : null;
        if (marker) {
          j += marker[0].length;
          continue;
        }
        break;
      }
      const next = line[j];
      if (j > i + 1 && next !== undefined && next >= 'A' && next <= 'Z') {
        sentences.push(line.slice(start, j).trim());
        start = j;
        i = j;
        continue;
      }
    }
    i++;
  }
  const rest = line.slice(start).trim();
  if (rest) sentences.push(rest);
  return sentences;
}

// Models sometimes write a citation as <P1>, (P1) or [P1, P2]. Rewritten to
// [P1][P2] so they are verified like any other citation. Each pattern has a
// fixed prefix and bounded runs, so matching is linear.
function normalizeMarkers(text: string): string {
  return text
    .replace(/<(P\d{1,4})>/g, '[$1]')
    .replace(/\((P\d{1,4})\)/g, '[$1]')
    .replace(/\[(P\d{1,4}(?: ?, ?P\d{1,4}){1,9})\]/g, (_m, list: string) =>
      list
        .split(',')
        .map((id) => `[${id.trim()}]`)
        .join('')
    )
    // "as shown in the <P1>" uses the citation as a word; keep the sentence
    // readable once the marker is removed
    .replace(/ the (?=\[P\d)/g, ' the source ');
}

// Every sentence of the draft with the passages it cites. A sentence with no
// markers of its own takes those of the next marked sentence in the same
// paragraph.
function draftSentences(draft: string): { raw: string; citations: string[] }[] {
  const result: { raw: string; citations: string[] }[] = [];
  // a model stuck in a loop repeats a sentence; it is checked once
  const seen = new Set<string>();
  for (const line of draft.split('\n')) {
    const paragraph = normalizeMarkers(line.replace(/^\s*(?:[-*•]|\d{1,3}[.)])\s+/, '').trim());
    if (paragraph.length === 0) continue;
    const sentences = splitSentences(paragraph).map((raw) => ({
      raw,
      citations: [...new Set([...raw.matchAll(MARKER)].map((m) => m[1]))]
    }));
    let inherited: string[] = [];
    for (let k = sentences.length - 1; k >= 0; k--) {
      if (sentences[k].citations.length > 0) inherited = sentences[k].citations;
      else sentences[k].citations = inherited;
    }
    for (const sentence of sentences) {
      const key = sentence.raw.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      result.push(sentence);
    }
  }
  return result;
}

/**
 * Checks a draft written from evidence passages, one sentence per line, each
 * ending in markers such as [P1][P3] that name the passages it came from.
 * Smaller models often write a paragraph with the markers only at its end;
 * the paragraph is split into sentences and each is checked on its own
 * against the passages the paragraph cites.
 *
 * A sentence is kept only if:
 * - it cites at least one passage, and every passage it cites was actually
 *   supplied (an invented citation removes the whole sentence);
 * - enough of its content words appear in the cited passages that the claim
 *   plausibly comes from them;
 * - every number in it appears in the cited passages (verifyNumbers);
 * - it does not negate, reverse a comparison, swap a number between groups
 *   or change a dose schedule relative to those passages (checkSupport).
 *
 * Removed sentences are returned with the reason, for the self-check retry
 * and the audit log; they are never shown to the doctor.
 */
export function verifyDraft(
  draft: string,
  passages: Map<string, string>,
  options: CitationVerifierOptions = {}
): DraftVerification {
  const minSupport = options.minSupport ?? DEFAULT_MIN_SUPPORT;
  const kept: VerifiedSentence[] = [];
  const removed: RemovedSentence[] = [];

  for (const { raw, citations } of draftSentences(draft)) {
    if (raw.length > MAX_SENTENCE_LENGTH) {
      removed.push({ text: raw.slice(0, 200), reason: 'sentence too long to verify' });
      continue;
    }
    // collapse whitespace left by the markers without a backtracking regex
    const text = raw
      .replace(MARKER, ' ')
      .split(/\s/)
      .filter((word) => word.length > 0)
      .join(' ')
      .replaceAll(' .', '.')
      .replaceAll(' ,', ',')
      .replaceAll(' ;', ';')
      .replaceAll(' :', ':');
    if (text.length === 0) continue;

    if (citations.length === 0) {
      removed.push({ text, reason: 'no citation' });
      continue;
    }
    const unknown = citations.filter((id) => !passages.has(id));
    if (unknown.length > 0) {
      removed.push({ text, reason: `cites passages that were not supplied: ${unknown.join(', ')}` });
      continue;
    }

    const citedTexts = citations.map((id) => passages.get(id) as string);
    const sourceTerms = new Set(citedTexts.flatMap((t) => queryTerms(t)));
    // 'source' may have been added by normalizeMarkers, so it is not scored
    const terms = queryTerms(text).filter((t) => !/^\d/.test(t) && t !== 'source');
    const supported = terms.filter((t) => sourceTerms.has(t)).length;
    if (terms.length > 0 && supported / terms.length < minSupport) {
      removed.push({
        text,
        reason: `only ${supported} of ${terms.length} content words appear in the cited passages`
      });
      continue;
    }

    const numbers = verifyNumbers(text, citedTexts);
    if (!numbers.ok) {
      removed.push({ text, reason: numbers.reason });
      continue;
    }

    // sharing the source's words is not enough: the sentence must not
    // negate, reverse or re-pair what the source says
    const support = checkSupport(text, citedTexts);
    if (!support.ok) {
      removed.push({ text, reason: support.reason });
      continue;
    }

    kept.push({ text, citations });
  }
  return { kept, removed };
}
