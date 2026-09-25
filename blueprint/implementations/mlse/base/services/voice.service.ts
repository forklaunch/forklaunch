import { AnswerResponseDto } from '@forklaunch/interfaces-mlse/types';

export type IdentifierRemoval = {
  text: string;
  // how many identifiers were replaced
  removed: number;
};

const REMOVED = '[removed]';
const MAX_TRANSCRIPT_LENGTH = 2000;

// Each pattern has a fixed anchor and bounded runs, so matching is linear.
// Clinical numbers (doses, weights, ages) are kept: the query classifier
// needs them to route "80 kg, how much propofol" to the boundary path.
const IDENTIFIER_PATTERNS: RegExp[] = [
  // email addresses
  /[a-z0-9._%+-]{1,64}@[a-z0-9-]{1,63}(?:\.[a-z0-9-]{1,63}){1,5}/gi,
  // dates: 12/03/1961, 12-03-61, 1961-03-12
  /\b\d{1,2}[/.-]\d{1,2}[/.-]\d{2,4}\b/g,
  /\b\d{4}-\d{1,2}-\d{1,2}\b/g,
  // phone numbers: +1 555 123 4567, (555) 123-4567
  /(?:\+\d{1,3}[ -]?)?\(?\d{3}\)?[ -]\d{3}[ -]\d{4}\b/g,
  // record, bed and room numbers named as such
  /\b(?:mrn|nhs|medical record|record|hospital number|patient id|id|bed|room|ward)(?: number| no\.?)?[ :#]{0,3}[a-z]?\d{1,12}\b/gi,
  // long digit runs (record numbers read out without a label)
  /\b\d{6,20}\b/g,
  // a title followed by a capitalised name: Mr Smith, Mrs. Anne Jones
  /\b(?:Mr|Mrs|Ms|Miss|Mx|Dr|Prof)\.? [A-Z][a-z'-]{1,30}(?: [A-Z][a-z'-]{1,30})?/g,
  // "patient's name is John", "named John Smith", "called Anne"
  /\b(?:name is|named|called)\s{1,3}[A-Z][a-z'-]{1,30}(?: [A-Z][a-z'-]{1,30})?/g
];

/**
 * Removes spoken identifiers from a voice transcript before it is searched,
 * logged or stored: names after a title or "name is", record, bed and room
 * numbers, dates, phone numbers and emails. Rule-based, so it errs on the
 * side of removing; it reduces what reaches MLSE but does not make an open
 * microphone safe, which is why voice is off by default and enabled per care
 * area by the hospital.
 */
export function removeIdentifiers(transcript: string): IdentifierRemoval {
  let text = transcript.slice(0, MAX_TRANSCRIPT_LENGTH);
  let removed = 0;
  for (const pattern of IDENTIFIER_PATTERNS) {
    text = text.replace(pattern, () => {
      removed++;
      return REMOVED;
    });
  }
  return { text, removed };
}

const MAX_SPOKEN_SENTENCES = 3;
const MAX_SPOKEN_WORDS = 70;

/**
 * A short text to read aloud in the operating room. Fixed messages are read
 * as written; for an answer, the first verified sentence of the first
 * answered sections, then a pointer to the sources on screen. Citations are
 * not read out, but every sentence spoken is one that passed verification
 * and is shown with its sources.
 */
export function spokenSummary(answer: AnswerResponseDto): string {
  if (answer.kind !== 'answer' && answer.kind !== 'label_range') {
    return answer.message ?? 'No answer is available.';
  }
  const sentences: string[] = [];
  if (answer.kind === 'label_range') {
    sentences.push('The label dosing section is on screen, quoted as written.');
  } else {
    for (const section of answer.sections) {
      if (section.status === 'answered' && section.sentences.length > 0) {
        sentences.push(section.sentences[0].text);
      }
      if (sentences.length >= MAX_SPOKEN_SENTENCES) break;
    }
  }
  if (sentences.length === 0) {
    return 'MLSE found no verified answer in its sources.';
  }

  const words: string[] = [];
  for (const sentence of sentences) {
    const next = sentence.split(/\s/).filter((w) => w.length > 0);
    if (words.length > 0 && words.length + next.length > MAX_SPOKEN_WORDS) break;
    words.push(...next.slice(0, MAX_SPOKEN_WORDS));
  }
  const sourceCount = answer.sources.length;
  return `${words.join(' ')} ${sourceCount === 1 ? 'The source is' : `All ${sourceCount} sources are`} on screen.`;
}

// Care areas are chosen by each hospital, e.g. operating_room or emergency.
export function isValidCareArea(area: string): boolean {
  return /^[a-z][a-z_]{1,39}$/.test(area);
}
