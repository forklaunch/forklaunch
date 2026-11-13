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
//
// The generic name decides. Many store brands are named after what they
// treat ("Pain Relief", "Sleep Aid", "Allergy Relief"), so a brand name in
// the query is trusted only when it is the only drug the query can mean: no
// label matched by its generic name, every word the query could name a drug
// by is part of the brand, and every label carrying that brand is the same
// generic drug. Otherwise nothing is quoted.

import { queryTerms } from './ranking.service';

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
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()
    .split(' ')
    .filter(Boolean);
}

interface LabelName {
  // the generic name, and without its salt when that differs
  generic: string[];
  // the brand name, when the title carries one besides the generic
  brand: string[];
}

function nameForms(raw: string): string[] {
  const nameWords = words(raw);
  if (nameWords.length === 0) return [];
  const forms = [nameWords.join(' ')];
  const core = nameWords.filter((w) => !SALT_WORDS.has(w));
  if (core.length > 0 && core.length < nameWords.length)
    forms.push(core.join(' '));
  return forms;
}

/**
 * The names on a label title as MLSE's fetchers write it: "Brand (generic)
 * — Manufacturer", or "Name — Manufacturer" when the label has one name
 * (the generic, or a brand spelled the same as it).
 */
function parseLabelName(title: string): LabelName {
  const name = title.split(' — ')[0].trim();
  const paren = /^(.*)\(([^()]*)\)\s*$/.exec(name);
  if (!paren) return { generic: nameForms(name), brand: [] };
  const generic = nameForms(paren[2]);
  return {
    generic,
    brand: nameForms(paren[1]).filter((form) => !generic.includes(form))
  };
}

/** Every name a label title carries: brand first, then generic forms. */
export function labelNames(title: string): string[] {
  const { generic, brand } = parseLabelName(title);
  return [...new Set([...brand, ...generic])];
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

// the drug a label is for: its generic name without the salt
function drugKey(label: LabelName): string {
  return label.generic[label.generic.length - 1] ?? '';
}

/** True when the query names this label's drug by its generic name. */
export function labelIsForQueryDrug(title: string, query: string): boolean {
  const phrases = queryPhrases(query);
  return parseLabelName(title).generic.some((name) => phrases.has(name));
}

/**
 * The labels whose dosing may be quoted for this query. `drugTerms` are the
 * words the query could name a drug by (the classifier's subject terms); a
 * brand-name match is trusted only when the brand accounts for all of them.
 */
export function labelsForQueryDrug<T extends { title: string }>(
  labels: T[],
  query: string,
  drugTerms: string[]
): T[] {
  const phrases = queryPhrases(query);
  const parsed = labels.map((label) => ({
    label,
    name: parseLabelName(label.title)
  }));

  const byGeneric = parsed.filter(({ name }) =>
    name.generic.some((n) => phrases.has(n))
  );
  if (byGeneric.length > 0) return byGeneric.map(({ label }) => label);

  const byBrand = parsed
    .map(({ label, name }) => ({
      label,
      name,
      brand: name.brand.find((n) => phrases.has(n))
    }))
    .filter(
      (match): match is typeof match & { brand: string } =>
        match.brand !== undefined
    );
  if (byBrand.length === 0) return [];

  // "ibuprofen for pain relief": "ibuprofen" is not part of the brand
  // "Pain Relief", so the query may be about another drug
  // (both sides as the classifier writes terms: "lantus" is "lantu")
  const brandTerms = new Set(
    queryTerms(byBrand.map(({ brand }) => brand).join(' '))
  );
  if (drugTerms.some((term) => !brandTerms.has(term))) return [];

  // "Sleep Aid" is doxylamine on one label and diphenhydramine on another
  const drugs = new Set(byBrand.map(({ name }) => drugKey(name)));
  if (drugs.size !== 1) return [];
  return byBrand.map(({ label }) => label);
}

/**
 * The label section that holds dosing. "Overdosage" also contains
 * "dosage", and quoting it as the dose would be dangerous.
 */
export function isDosingSection(sectionPath: string): boolean {
  return /^dosage and administration\b/i.test(sectionPath.trim());
}
