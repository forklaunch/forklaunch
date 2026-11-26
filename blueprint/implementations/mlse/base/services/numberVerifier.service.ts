import { extractQuantities } from './quantityExtractor.service';

export type NumberCheck = { ok: true } | { ok: false; reason: string };

// Plain numbers as written ("20", "2.5", "1,000"); one bounded character
// class per run, so matching is linear.
function numbersIn(text: string): string[] {
  return [...text.matchAll(/\d[\d,]*(?:\.\d+)?/g)].map((m) => normalizeNumber(m[0]));
}

function normalizeNumber(raw: string): string {
  const withoutGrouping = raw.replace(/,(?=\d{3}(?:\D|$))/g, '');
  const value = Number(withoutGrouping.replace(/,/g, '.'));
  return Number.isFinite(value) ? String(value) : withoutGrouping;
}

/**
 * Every number in a drafted sentence must appear in the passages it cites,
 * and every measurement (a number with a unit) must match one of theirs in
 * value and unit. A sentence that says "30 mL" when the source says "20 mL",
 * or "2 days" when the source says "2 hours", fails. The model is never
 * trusted to calculate: a total, average or conversion it worked out itself
 * is not in any source and fails the same way.
 */
export function verifyNumbers(sentence: string, citedTexts: string[]): NumberCheck {
  const sourceNumbers = new Set(citedTexts.flatMap(numbersIn));
  for (const number of numbersIn(sentence)) {
    if (!sourceNumbers.has(number)) {
      return { ok: false, reason: `number ${number} is not in the cited passages` };
    }
  }

  const sourceQuantities = citedTexts.flatMap((text) => extractQuantities(text));
  for (const quantity of extractQuantities(sentence)) {
    const matched = sourceQuantities.some(
      (q) => q.low === quantity.low && q.high === quantity.high && q.unit === quantity.unit
    );
    if (!matched) {
      return { ok: false, reason: `"${quantity.raw}" does not match a measurement in the cited passages` };
    }
  }
  return { ok: true };
}
