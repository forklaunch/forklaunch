// Decides whether a drug label is for a drug the query names. A label's
// dosing section is quoted to a doctor as written, so quoting the wrong
// drug's label is the worst answer MLSE can give: "what dose of insulin
// should I take for a blood sugar of 300" must never quote "Childrens
// Cetirizine Sugar Free" because both contain "sugar".
//
// A label matches only when a whole phrase of the query equals one of the
// label's names: the generic name, the generic name without its salt
// ("morphine" for morphine sulfate), or the brand name. One shared word is
// never enough, and an ambiguous name ("insulin", for insulin glargine,
// lispro, aspart...) matches nothing, so nothing is quoted.

// Salt and dosage-form words that do not change which drug a name is
// ("warfarin sodium" is warfarin, "cefazolin for injection, USP" is
// cefazolin, "propofol injectable emulsion" is propofol).
const SALT_WORDS = new Set([
  'usp',
  'for',
  'injection',
  'injectable',
  'emulsion',
  'solution',
  'suspension',
  'oral',
  'tablet',
  'tablets',
  'capsule',
  'capsules',
  'extended',
  'delayed',
  'release',
  'er',
  'xr',
  'film',
  'coated',
  'chewable',
  'powder',
  'cream',
  'ointment',
  'gel',
  'sodium',
  'potassium',
  'calcium',
  'magnesium',
  'hydrochloride',
  'hcl',
  'sulfate',
  'sulphate',
  'acetate',
  'citrate',
  'phosphate',
  'maleate',
  'tartrate',
  'succinate',
  'mesylate',
  'besylate',
  'bromide',
  'chloride',
  'fumarate',
  'monohydrate',
  'dihydrate',
  'trihydrate',
  'anhydrous'
]);

const MAX_PHRASE_WORDS = 4;

function words(text: string): string[] {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim().split(' ').filter(Boolean);
}

/**
 * The names on a label title as MLSE's fetchers write it: "Brand (generic)
 * — Manufacturer", or "Name — Manufacturer" when the label has one name.
 */
export function labelNames(title: string): string[] {
  const name = title.split(' — ')[0].trim();
  const paren = /^(.*)\(([^()]*)\)\s*$/.exec(name);
  const names = paren ? [paren[1], paren[2]] : [name];
  const out = new Set<string>();
  for (const raw of names) {
    const nameWords = words(raw);
    if (nameWords.length === 0) continue;
    out.add(nameWords.join(' '));
    const core = nameWords.filter((w) => !SALT_WORDS.has(w));
    if (core.length > 0 && core.length < nameWords.length) out.add(core.join(' '));
  }
  return [...out];
}

/** Every run of one to four consecutive words in the query. */
function queryPhrases(query: string): Set<string> {
  const queryWords = words(query);
  const phrases = new Set<string>();
  for (let i = 0; i < queryWords.length; i++) {
    for (let n = 1; n <= MAX_PHRASE_WORDS && i + n <= queryWords.length; n++) {
      phrases.add(queryWords.slice(i, i + n).join(' '));
    }
  }
  return phrases;
}

/** True when the query names the drug this label is for. */
export function labelIsForQueryDrug(title: string, query: string): boolean {
  const phrases = queryPhrases(query);
  return labelNames(title).some((name) => phrases.has(name));
}

/**
 * The label section that holds dosing. "Overdosage" also contains
 * "dosage", and quoting it as the dose would be dangerous.
 */
export function isDosingSection(sectionPath: string): boolean {
  return /^dosage and administration\b/i.test(sectionPath.trim());
}
