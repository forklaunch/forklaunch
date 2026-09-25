export type ExtractedQuantity = {
  // the number as written, e.g. "20 mL" or "12–14 mmHg"
  raw: string;
  low: number;
  high: number;
  unit: string;
  // statistic named just before the number, if any
  statistic?: 'median' | 'mean' | 'range' | 'iqr' | 'percent';
  // the sentence it came from, kept so a reviewer sees the context
  sentence: string;
};

// Units MLSE extracts, longest first so "mg/kg" wins over "mg". Only units a
// procedure or dosing answer needs; counts without units are ignored.
const UNITS: [pattern: string, normalized: string][] = [
  ['mg/kg', 'mg/kg'],
  ['mcg/kg', 'mcg/kg'],
  ['mmHg', 'mmHg'],
  ['mL', 'mL'],
  ['ml', 'mL'],
  ['mg', 'mg'],
  ['mcg', 'mcg'],
  ['g', 'g'],
  ['L', 'L'],
  ['units', 'units'],
  ['minutes', 'min'],
  ['minute', 'min'],
  ['min', 'min'],
  ['hours', 'h'],
  ['hour', 'h'],
  ['days', 'days'],
  ['day', 'days'],
  ['%', '%']
];

const STATISTICS: [word: string, statistic: ExtractedQuantity['statistic']][] = [
  ['median', 'median'],
  ['mean', 'mean'],
  ['average', 'mean'],
  ['iqr', 'iqr'],
  ['interquartile', 'iqr'],
  ['range', 'range']
];

const isDigit = (c: string | undefined) => c !== undefined && c >= '0' && c <= '9';
const isLetter = (c: string | undefined) => c !== undefined && /[a-zA-Z]/.test(c);

/**
 * Finds numbers with medical units in text, e.g. "median blood loss was
 * 20 mL", "1.5–2.5 mg/kg", "12 to 14 mmHg", "18%". A hand-written scanner
 * rather than one large regular expression, so it runs in linear time on any
 * input (source text is external data).
 *
 * Extracted values are candidates for review, never facts: population and
 * technique are left for a clinician or the verification step to confirm.
 */
export function extractQuantities(text: string): ExtractedQuantity[] {
  const results: ExtractedQuantity[] = [];
  const sentences = splitSentences(text);

  for (const sentence of sentences) {
    let i = 0;
    while (i < sentence.length) {
      if (!isDigit(sentence[i]) || isDigit(sentence[i - 1]) || isLetter(sentence[i - 1]) || sentence[i - 1] === '.') {
        i++;
        continue;
      }
      const start = i;
      const first = readNumber(sentence, i);
      i = first.end;

      // optional range: "12–14", "12-14", "12 to 14"
      let high = first.value;
      let j = skipSpaces(sentence, i);
      const dash = sentence[j] === '–' || sentence[j] === '-' ? 1 : sentence.startsWith('to ', j) ? 3 : 0;
      if (dash) {
        const k = skipSpaces(sentence, j + dash);
        if (isDigit(sentence[k])) {
          const second = readNumber(sentence, k);
          high = second.value;
          i = second.end;
          j = skipSpaces(sentence, i);
        }
      }

      const unit = UNITS.find(([pattern]) => sentence.startsWith(pattern, j) && !isLetter(sentence[j + pattern.length]) && !(pattern !== '%' && sentence[j + pattern.length] === '/'));
      if (!unit || (j > i + 1)) {
        continue;
      }
      const end = j + unit[0].length;
      const before = sentence.slice(Math.max(0, start - 40), start).toLowerCase();
      const statistic =
        unit[1] === '%'
          ? 'percent'
          : STATISTICS.find(([word]) => before.includes(word))?.[1];

      results.push({
        raw: sentence.slice(start, end).trim(),
        low: Math.min(first.value, high),
        high: Math.max(first.value, high),
        unit: unit[1],
        statistic,
        sentence
      });
      i = end;
    }
  }
  return results;
}

function readNumber(text: string, start: number): { value: number; end: number } {
  let i = start;
  while (isDigit(text[i])) i++;
  if (text[i] === '.' && isDigit(text[i + 1])) {
    i++;
    while (isDigit(text[i])) i++;
  }
  return { value: Number(text.slice(start, i)), end: i };
}

function skipSpaces(text: string, start: number): number {
  let i = start;
  while (text[i] === ' ' || text[i] === ' ') i++;
  return i;
}

function splitSentences(text: string): string[] {
  const sentences: string[] = [];
  let start = 0;
  for (let i = 0; i < text.length - 1; i++) {
    if ((text[i] === '.' || text[i] === ';') && text[i + 1] === ' ' && !isDigit(text[i - 1]) ) {
      sentences.push(text.slice(start, i + 1).trim());
      start = i + 2;
    }
  }
  sentences.push(text.slice(start).trim());
  return sentences.filter(Boolean);
}
