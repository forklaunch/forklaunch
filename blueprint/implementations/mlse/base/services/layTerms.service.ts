// Everyday terms doctors and patients type, mapped to the clinical terms the
// literature indexes them under ("heart attack" is indexed as myocardial
// infarction). MeSH entry terms cover many of these once the MeSH file is
// loaded; this list makes the common ones work without it.
//
// STATUS: DRAFT. Engineering list; to be reviewed by a clinician with the
// question frameworks.
const LAY_TERMS: [lay: string[], clinical: string][] = [
  [['heart attack', 'heart attacks'], 'myocardial infarction'],
  // guidelines cover heart attack under the wider acute coronary syndrome
  [['heart attack', 'heart attacks'], 'acute coronary syndrome'],
  [['brain attack'], 'stroke'],
  [['mini stroke', 'ministroke'], 'transient ischemic attack'],
  [['high blood pressure'], 'hypertension'],
  [['low blood pressure'], 'hypotension'],
  [['high cholesterol'], 'hypercholesterolemia'],
  [['high blood sugar'], 'hyperglycemia'],
  [['low blood sugar'], 'hypoglycemia'],
  [['sugar disease', 'sugar problem'], 'diabetes mellitus'],
  [['irregular heartbeat', 'irregular heart beat'], 'arrhythmia'],
  [['heart failure'], 'heart failure'],
  [['blood clot', 'blood clots'], 'thrombosis'],
  [['clot in the lung', 'lung clot', 'blood clot in lung'], 'pulmonary embolism'],
  [['clot in the leg', 'leg clot'], 'deep vein thrombosis'],
  [['blood thinner', 'blood thinners'], 'anticoagulants'],
  [['heartburn', 'acid reflux'], 'gastroesophageal reflux disease'],
  [['stomach ulcer', 'stomach ulcers'], 'peptic ulcer'],
  [['kidney stone', 'kidney stones'], 'nephrolithiasis'],
  [['gallstone', 'gallstones'], 'cholelithiasis'],
  [['gallbladder removal', 'gall bladder removal'], 'cholecystectomy'],
  [['kidney failure'], 'renal insufficiency'],
  [['piles'], 'hemorrhoids'],
  [['fits'], 'seizures'],
  [['flu'], 'influenza'],
  [['tb'], 'tuberculosis'],
  [['asthma attack'], 'asthma exacerbation'],
  [['broken bone', 'broken bones'], 'fracture'],
  [['womb removal'], 'hysterectomy'],
  [['c section', 'c-section', 'caesarean', 'cesarean'], 'cesarean section'],
  // US papers say "cesarean delivery", British ones "caesarean section"
  [['c section', 'c-section', 'caesarean', 'cesarean'], 'cesarean delivery'],
  [['c section', 'c-section', 'caesarean', 'cesarean'], 'caesarean section'],
  [['bone marrow transplant', 'bone marrow transplantation'], 'hematopoietic stem cell transplantation'],
  [['water on the brain'], 'hydrocephalus'],
  [['slipped disc', 'slipped disk'], 'intervertebral disc displacement'],
  [['pink eye'], 'conjunctivitis'],
  [['chicken pox', 'chickenpox'], 'varicella'],
  [['whooping cough'], 'pertussis']
];

function normalize(text: string): string {
  return ` ${text.toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim()} `;
}

// Also matches the words run together ("heartattack") and with a hyphen.
const ENTRIES = LAY_TERMS.flatMap(([lays, clinical]) =>
  lays.flatMap((lay) => {
    const spaced = normalize(lay);
    const joined = ` ${spaced.trim().replaceAll(' ', '')} `;
    return [{ form: spaced, clinical }, { form: joined, clinical }];
  })
);

// Question words that make a literature search find nothing: PubMed finds
// no papers for "Does aspirin help prevent a first heart attack in people
// over 60?" and thousands for "aspirin prevent myocardial infarction".
const QUESTION_WORDS = new Set([
  'a', 'an', 'the', 'is', 'are', 'was', 'were', 'be', 'do', 'does', 'did', 'can', 'could', 'should', 'would',
  'will', 'what', 'which', 'who', 'when', 'how', 'why', 'there', 'any', 'help', 'helps', 'really', 'me', 'i',
  'my', 'you', 'your', 'people', 'person', 'over', 'under', 'in', 'of', 'for', 'to', 'with', 'on', 'at', 'it'
]);

/**
 * The query as a literature search: everyday terms replaced by their
 * clinical term ("heart attack" -> myocardial infarction) and question
 * words and bare numbers left out. A query that is only an everyday term
 * becomes its clinical term.
 */
export function literatureSearchTerm(query: string): string {
  let text = normalize(query.slice(0, 2000));
  const found = ENTRIES.filter((entry) => text.includes(entry.form)).sort((a, b) => b.form.length - a.form.length);
  // each everyday term once, by its first clinical term
  const replaced = new Set<string>();
  for (const entry of found) {
    if (!text.includes(entry.form) || replaced.has(entry.form)) continue;
    replaced.add(entry.form);
    // "cesarean section" already names its clinical term
    if (text.includes(` ${entry.clinical} `)) continue;
    text = text.replace(entry.form, ` ${entry.clinical} `);
  }
  const words = text.trim().split(/\s+/).filter((w) => w && !QUESTION_WORDS.has(w) && !/^\d+$/.test(w));
  return [...new Set(words)].join(' ') || query.trim();
}

/**
 * Every everyday and clinical term in the list, for spelling suggestions and
 * completion as a query is typed.
 */
export function knownTerms(): string[] {
  return [...new Set(LAY_TERMS.flatMap(([lays, clinical]) => [...lays, clinical]))];
}

/**
 * The clinical terms for the everyday terms a query contains, most specific
 * (longest match) first. Empty when the query already uses clinical terms.
 */
export function clinicalTermsFor(query: string): string[] {
  const text = normalize(query.slice(0, 2000));
  const found = ENTRIES.filter((entry) => text.includes(entry.form)).sort(
    (a, b) => b.form.length - a.form.length
  );
  return [...new Set(found.map((entry) => entry.clinical))];
}
