// Checks that a drafted sentence says what its sources say, beyond sharing
// their words. Word overlap alone keeps "Aspirin did not reduce mortality"
// against a source saying it did, because every word is in the source. These
// rules reject the common ways a sentence can reuse a source's words and
// still contradict it:
//
// - negation: a word negated in one and not the other ("did not reduce")
// - opposites: "recommended" where the source says "contraindicated"
// - comparisons: "A superior to B" where the source says B superior to A
// - numbers bound to groups: "20% with aspirin" where the source gives 20%
//   for placebo and another figure for aspirin
// - dose schedules: "500 mg every 8 hours" where the source gives 500 mg
//   twice daily
//
// They are rules, not understanding: they catch these patterns, not every
// misreading. Linear scans over tokens, no backtracking expressions.

export type SupportCheck = { ok: true } | { ok: false; reason: string };

const STOP = new Set([
  'a', 'an', 'the', 'and', 'or', 'of', 'in', 'on', 'at', 'to', 'for', 'with', 'by', 'from', 'as', 'that', 'this',
  'is', 'was', 'were', 'are', 'be', 'been', 'being', 'it', 'its', 'has', 'have', 'had', 'do', 'does', 'did',
  'can', 'could', 'may', 'might', 'should', 'would', 'will', 'there', 'than', 'which', 'who', 'also', 'both',
  'significantly', 'statistically', 'clinically', 'much', 'any', 'all', 'yet', 'always', 'still', 'only'
]);
const NEGATIONS = new Set(['not', 'no', 'never', 'neither', 'nor', 'without']);
// generic words between a number and the group it belongs to
const GROUP_FILLER = new Set(['group', 'groups', 'arm', 'arms', 'patients', 'patient', 'participants', 'cohort', 'among', 'receiving', 'treated', 'given', 'those', 'who', 'subjects']);
const UNIT_TOKENS = new Set(['mg', 'g', 'mcg', 'kg', 'ml', 'l', 'h', 'hr', 'hrs', 'hour', 'hours', 'min', 'minute', 'minutes', 'day', 'days', 'week', 'weeks', 'month', 'months', 'year', 'years', 'units', 'mmhg']);

// up: higher/better; down: lower/worse
const COMPARATORS = new Map<string, 1 | -1>([
  ['superior', 1], ['inferior', -1], ['better', 1], ['worse', -1], ['higher', 1], ['lower', -1],
  ['greater', 1], ['larger', 1], ['smaller', -1], ['more', 1], ['less', -1], ['fewer', -1],
  ['longer', 1], ['shorter', -1]
]);
const COMPARISON_LINKS = new Set(['than', 'to', 'compared', 'versus', 'vs']);

const OPPOSITES: [string, string][] = [
  ['recommended', 'contraindicated'],
  ['indicated', 'contraindicated'],
  ['safe', 'unsafe'],
  ['effective', 'ineffective'],
  ['increased', 'decreased'],
  ['increased', 'reduced'],
  ['improved', 'worsened'],
  ['benefit', 'harm']
  // superior/inferior and higher/lower are left to the comparison rule:
  // "A inferior to B" is a faithful way to say "B superior to A"
];

export function stem(word: string): string {
  for (const suffix of ['ations', 'ation', 'ing', 'ed', 'es', 'ly', 'e', 's']) {
    if (word.length - suffix.length >= 4 && word.endsWith(suffix)) return word.slice(0, -suffix.length);
  }
  return word;
}

const OPPOSITE_STEMS = OPPOSITES.map(([a, b]) => [stem(a), stem(b)] as const);

function tokens(text: string): string[] {
  const normalized = text
    .toLowerCase()
    .replace(/n't\b/g, ' not')
    .replace(/\bcannot\b/g, 'can not')
    .replace(/\bnot only\b/g, ' ');
  // punctuation is kept as tokens so a negation stops at a clause boundary
  return normalized.match(/\d+(?:\.\d+)?|%|[a-z]+|[.;,]/g) ?? [];
}

function sentencesOf(text: string): string[] {
  const out: string[] = [];
  let start = 0;
  for (let i = 0; i < text.length - 1; i++) {
    const c = text[i];
    if ((c === '.' || c === ';' || c === '!' || c === '?') && text[i + 1] === ' ' && !(c === '.' && /\d/.test(text[i - 1] ?? '') && /\d/.test(text[i + 2] ?? ''))) {
      out.push(text.slice(start, i + 1));
      start = i + 2;
    }
  }
  out.push(text.slice(start));
  return out.map((s) => s.trim()).filter(Boolean);
}

const isPunctuation = (t: string) => t === '.' || t === ';' || t === ',';
const isContent = (t: string) =>
  !STOP.has(t) && !NEGATIONS.has(t) && !/^\d/.test(t) && t !== '%' && !isPunctuation(t);

// the content words a negation applies to: the first one or two after it
function negatedStems(toks: string[]): Set<string> {
  const out = new Set<string>();
  for (let i = 0; i < toks.length; i++) {
    if (!NEGATIONS.has(toks[i])) continue;
    let taken = 0;
    for (let j = i + 1; j < toks.length && j <= i + 4 && taken < 2; j++) {
      if (NEGATIONS.has(toks[j]) || isPunctuation(toks[j])) break;
      if (isContent(toks[j])) {
        out.add(stem(toks[j]));
        taken += 1;
      }
    }
  }
  return out;
}

// the cited sentences that share the most words with the claim
function closestSentences(claim: string[], citedTexts: string[]): string[][] {
  const claimStems = new Set(claim.filter(isContent).map(stem));
  let best: string[][] = [];
  let bestScore = 0;
  for (const sentence of citedTexts.flatMap(sentencesOf)) {
    const toks = tokens(sentence);
    const score = new Set(toks.filter(isContent).map(stem).filter((s) => claimStems.has(s))).size;
    if (score > bestScore) {
      best = [toks];
      bestScore = score;
    } else if (score === bestScore && score > 0) {
      best.push(toks);
    }
  }
  return best;
}

function negationAgrees(claim: string[], source: string[]): string | undefined {
  const claimStems = new Set(claim.filter(isContent).map(stem));
  const sourceStems = new Set(source.filter(isContent).map(stem));
  const claimNegated = negatedStems(claim);
  const sourceNegated = negatedStems(source);
  for (const s of claimNegated) {
    if (sourceStems.has(s) && !sourceNegated.has(s)) return `negates "${s}", which the source does not`;
  }
  for (const s of sourceNegated) {
    if (claimStems.has(s) && !claimNegated.has(s)) return `drops the source's negation of "${s}"`;
  }
  return undefined;
}

function oppositesAgree(claim: string[], source: string[]): string | undefined {
  // a negated word is not asserted: "not recommended" does not say recommended
  const negated = negatedStems(claim);
  const claimStems = new Set(claim.map(stem).filter((s) => !negated.has(s)));
  const sourceStems = new Set(source.map(stem));
  for (const [a, b] of OPPOSITE_STEMS) {
    if (claimStems.has(a) && sourceStems.has(b) && !sourceStems.has(a)) return `says "${a}" where the source says "${b}"`;
    if (claimStems.has(b) && sourceStems.has(a) && !sourceStems.has(b)) return `says "${b}" where the source says "${a}"`;
  }
  return undefined;
}

// "A superior to B" as (A, B, +1), normalized so the pair is ordered
type Comparison = { left: string; right: string; direction: 1 | -1 };

function comparisons(toks: string[]): Comparison[] {
  const out: Comparison[] = [];
  for (let c = 0; c < toks.length; c++) {
    const direction = COMPARATORS.get(toks[c]);
    if (!direction) continue;
    let link = -1;
    for (let j = c + 1; j <= c + 2 && j < toks.length; j++) {
      if (COMPARISON_LINKS.has(toks[j])) {
        link = j;
        break;
      }
    }
    if (link < 0) continue;
    let left: string | undefined;
    for (let j = c - 1; j >= 0 && j >= c - 4; j--) {
      if (isContent(toks[j])) {
        left = stem(toks[j]);
        break;
      }
    }
    let right: string | undefined;
    for (let j = link + 1; j < toks.length && j <= link + 4; j++) {
      if (toks[j] === 'with') continue;
      if (isContent(toks[j])) {
        right = stem(toks[j]);
        break;
      }
    }
    if (!left || !right || left === right) continue;
    out.push(left < right ? { left, right, direction } : { left: right, right: left, direction: direction === 1 ? -1 : 1 });
  }
  return out;
}

function comparisonsAgree(claim: string[], sources: string[][]): string | undefined {
  const sourceComparisons = sources.flatMap(comparisons);
  for (const c of comparisons(claim)) {
    const opposite = sourceComparisons.find((s) => s.left === c.left && s.right === c.right && s.direction !== c.direction);
    const same = sourceComparisons.find((s) => s.left === c.left && s.right === c.right && s.direction === c.direction);
    if (opposite && !same) return `reverses the comparison of "${c.left}" and "${c.right}"`;
  }
  return undefined;
}

// Each number (with its unit) and the group it is about: the first content
// word after it, past "in the ... group" filler.
function numberGroups(toks: string[]): { key: string; group: string }[] {
  const out: { key: string; group: string }[] = [];
  for (let i = 0; i < toks.length; i++) {
    if (!/^\d/.test(toks[i])) continue;
    let j = i + 1;
    let unit = '';
    if (toks[j] === '%' || UNIT_TOKENS.has(toks[j])) {
      unit = toks[j];
      j += 1;
    }
    while (UNIT_TOKENS.has(toks[j])) j += 1;
    for (let k = j; k < toks.length && k <= j + 4; k++) {
      if (/^\d/.test(toks[k])) break;
      if (isContent(toks[k]) && !GROUP_FILLER.has(toks[k])) {
        out.push({ key: `${Number(toks[i])}${unit}`, group: stem(toks[k]) });
        break;
      }
    }
  }
  return out;
}

function numbersBoundAlike(claim: string[], sources: string[][]): string | undefined {
  const sourcePairs = sources.flatMap(numberGroups);
  const sourceGroups = new Set(sourcePairs.map((p) => p.group));
  for (const { key, group } of numberGroups(claim)) {
    const groupsForNumber = new Set(sourcePairs.filter((p) => p.key === key).map((p) => p.group));
    // only decided when the source ties this number to another group and
    // gives the claim's group a different figure
    if (groupsForNumber.size > 0 && !groupsForNumber.has(group) && sourceGroups.has(group)) {
      return `ties ${key} to "${group}", which the source gives for "${[...groupsForNumber].join(', ')}"`;
    }
  }
  return undefined;
}

// "500 mg every 8 hours", "1 g twice daily", "250 mg q12h"
function doseSchedules(text: string): { dose: string; schedule: string }[] {
  const lower = text.toLowerCase();
  const out: { dose: string; schedule: string }[] = [];
  // A match may not start inside a number, and the digit runs are capped,
  // so a long run of digits with no unit costs linear time, not quadratic.
  const doseRe = /(?<![\d.])(\d{1,7}(?:\.\d{1,4})?)\s*(mg\/kg|mcg\/kg|mg|mcg|g|units|ml)\b/g;
  for (const m of lower.matchAll(doseRe)) {
    const after = lower.slice((m.index ?? 0) + m[0].length, (m.index ?? 0) + m[0].length + 40);
    const schedule = scheduleIn(after);
    if (schedule) out.push({ dose: `${Number(m[1])} ${m[2]}`, schedule });
  }
  return out;
}

// The schedule right after a dose: at most two plain words between them
// ("500 mg orally every 8 hours"), never another number, so one dose cannot
// take the next dose's schedule.
const LEAD = '^(?:[a-z]+\\s+){0,2}?';
const ABBREVIATIONS: Record<string, string> = {
  bid: 'twice daily',
  tid: 'three times daily',
  qid: 'four times daily',
  qd: 'once daily',
  daily: 'once daily'
};

function scheduleIn(text: string): string | undefined {
  const t = text.trimStart();
  const every = new RegExp(`${LEAD}every\\s+(\\d+(?:\\.\\d+)?)\\s*(hours?|hrs?|h|days?|weeks?)\\b`).exec(t);
  if (every) return `every ${Number(every[1])} ${every[2][0]}`;
  const q = new RegExp(`${LEAD}q(\\d+)h\\b`).exec(t);
  if (q) return `every ${Number(q[1])} h`;
  const times = new RegExp(`${LEAD}(once|twice|three times|four times)\\s+(?:a\\s+)?(daily|day|weekly|week)\\b`).exec(t);
  if (times) return `${times[1]} ${times[2].startsWith('week') ? 'weekly' : 'daily'}`;
  const abbreviation = new RegExp(`${LEAD}(bid|tid|qid|qd|daily)\\b`).exec(t);
  if (abbreviation) return ABBREVIATIONS[abbreviation[1]];
  return undefined;
}

function schedulesAgree(claim: string, citedTexts: string[]): string | undefined {
  const sourceSchedules = citedTexts.flatMap(doseSchedules);
  for (const { dose, schedule } of doseSchedules(claim)) {
    if (!sourceSchedules.some((s) => s.dose === dose && s.schedule === schedule)) {
      return `gives ${dose} ${schedule}, which no cited passage does`;
    }
  }
  return undefined;
}

/** Whether a sentence's meaning agrees with the passages it cites. */
export function checkSupport(sentence: string, citedTexts: string[]): SupportCheck {
  const claim = tokens(sentence);
  const closest = closestSentences(claim, citedTexts);
  if (closest.length > 0) {
    // a claim passes when at least one of the closest sentences agrees
    const negation = closest.map((s) => negationAgrees(claim, s));
    if (negation.every(Boolean)) return { ok: false, reason: negation[0] as string };
    const opposite = closest.map((s) => oppositesAgree(claim, s));
    if (opposite.every(Boolean)) return { ok: false, reason: opposite[0] as string };
  }
  const allSentences = citedTexts.flatMap(sentencesOf).map(tokens);
  const comparison = comparisonsAgree(claim, allSentences);
  if (comparison) return { ok: false, reason: comparison };
  const binding = numbersBoundAlike(claim, allSentences);
  if (binding) return { ok: false, reason: binding };
  const schedule = schedulesAgree(sentence, citedTexts);
  if (schedule) return { ok: false, reason: schedule };
  return { ok: true };
}
