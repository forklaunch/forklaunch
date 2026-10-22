import { createHash } from 'node:crypto';
import {
  DocumentSectionDto,
  PassageDto
} from '@forklaunch/interfaces-mlse/types';

export type ChunkOptions = {
  // upper bound on a passage's length; long enough to hold a full dosing
  // paragraph, short enough that a citation points at something specific
  maxChars?: number;
};

/**
 * Splits sections into passages at sentence boundaries, never across
 * sections, so every passage keeps the section it came from. A single
 * sentence longer than maxChars is split at word boundaries.
 */
export function chunkSections(
  sections: DocumentSectionDto[],
  { maxChars = 1200 }: ChunkOptions = {}
): PassageDto[] {
  const passages: PassageDto[] = [];
  let ordinal = 0;

  for (const section of sections) {
    let current = '';
    const flush = () => {
      const text = current.trim();
      if (text) {
        passages.push({ sectionPath: section.path, ordinal: ordinal++, text });
      }
      current = '';
    };

    for (const sentence of splitSentences(section.text)) {
      for (const piece of splitLong(sentence, maxChars)) {
        if (current && current.length + 1 + piece.length > maxChars) {
          flush();
        }
        current = current ? `${current} ${piece}` : piece;
      }
    }
    flush();
  }

  return passages;
}

// Sentence ends: '.', '!' or '?' followed by whitespace and an uppercase
// letter, digit or opening bracket. Abbreviations like "e.g. the" stay intact
// because a lowercase word follows them.
function splitSentences(text: string): string[] {
  const sentences: string[] = [];
  let start = 0;
  for (let i = 0; i < text.length - 2; i++) {
    const c = text[i];
    if (
      (c === '.' || c === '!' || c === '?') &&
      text[i + 1] === ' ' &&
      /[A-Z0-9([]/.test(text[i + 2])
    ) {
      sentences.push(text.slice(start, i + 1).trim());
      start = i + 2;
    }
  }
  const rest = text.slice(start).trim();
  if (rest) {
    sentences.push(rest);
  }
  return sentences.filter(Boolean);
}

function splitLong(sentence: string, maxChars: number): string[] {
  if (sentence.length <= maxChars) {
    return [sentence];
  }
  const pieces: string[] = [];
  let current = '';
  for (const word of sentence.split(' ')) {
    if (current && current.length + 1 + word.length > maxChars) {
      pieces.push(current);
      current = '';
    }
    // a single word longer than maxChars (a URL, a chemical name) is cut
    current = current ? `${current} ${word}` : word.slice(0, maxChars);
  }
  if (current) {
    pieces.push(current);
  }
  return pieces;
}

// Stable fingerprint of a document's content: unchanged documents are
// skipped on re-ingestion, changed ones get a new version.
export function contentHash(
  title: string,
  sections: DocumentSectionDto[]
): string {
  const hash = createHash('sha256');
  hash.update(title);
  for (const section of sections) {
    hash.update('\u0000');
    hash.update(section.path);
    hash.update('\u0000');
    hash.update(section.text);
  }
  return hash.digest('hex');
}
