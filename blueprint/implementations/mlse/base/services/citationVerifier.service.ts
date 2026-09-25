import { verifyNumbers } from './numberVerifier.service';
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

/**
 * Checks a draft written from evidence passages, one sentence per line, each
 * ending in markers such as [P1][P3] that name the passages it came from.
 *
 * A sentence is kept only if:
 * - it cites at least one passage, and every passage it cites was actually
 *   supplied (an invented citation removes the whole sentence);
 * - enough of its content words appear in the cited passages that the claim
 *   plausibly comes from them;
 * - every number in it appears in the cited passages (verifyNumbers).
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

  for (const line of draft.split('\n')) {
    const raw = line.replace(/^\s*(?:[-*•]|\d{1,3}[.)])\s+/, '').trim();
    if (raw.length === 0) continue;
    if (raw.length > MAX_SENTENCE_LENGTH) {
      removed.push({ text: raw.slice(0, 200), reason: 'sentence too long to verify' });
      continue;
    }

    const citations = [...new Set([...raw.matchAll(MARKER)].map((m) => m[1]))];
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
    const terms = queryTerms(text).filter((t) => !/^\d/.test(t));
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

    kept.push({ text, citations });
  }
  return { kept, removed };
}
