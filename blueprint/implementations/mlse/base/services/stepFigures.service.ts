import { ImageSearchService, MedicalImage } from './imageSearch.service';

// A caption that says how part of an operation is done
const STEP_WORDS =
  /\b(intra-?operative|operative|surgical (view|field|technique|step)|technique|steps?|incision|incised|dissect\w*|sutur\w*|clos(ure|ed|ing)|placed|placement|cut|resect\w*|ligat\w*|exposed|exposure|retract\w*|inserted|trocars?|ports?|stapl\w*|clips?|clipped|anastomos\w*|graft(ed|ing)?|harvest\w*|cannulat\w*|grasp\w*|exterioriz\w*|extract\w*|fixation|fixed|implanted|osteotomy|sternotomy|arthrotomy|uterotomy|delivered)\b/i;

// Imaging, laboratory and animal figures, and charts: not a view of the
// operation on a patient
const NOT_A_STEP =
  /\b(ultrasound\w*|ultrasonograph\w*|sonograph\w*|mri|magnetic resonance|computed tomography|ct|radiograph\w*|x-rays?|fluoroscop\w*|flow ?charts?|flow diagram|kaplan|histolog\w*|histopatholog\w*|stain\w*|microscop\w*|mice|mouse|rats?|rabbits?|pigs?|porcine|ovine|sheep|cadaver\w*|forest plot|graphs?|bar charts?|questionnaires?|survey|finite element|simulation)\b/i;

// After the operation: a healed wound, a follow-up visit
const AFTERWARDS =
  /\b(after (the )?(surgery|operation)|post-?operative(ly)?|follow-?up|heal(ed|ing)|(days?|weeks?|months?|years?) (after|later))\b/i;

// Words of a procedure's names that do not tell it from others
const GENERIC_NAME_WORDS = new Set([
  'section', 'delivery', 'laparoscopic', 'open', 'total', 'replacement', 'repair', 'artery', 'surgery', 'surgical', 'grafting'
]);

// "(A) ..." or "A The first cut. B ...": a figure in steps
const PANEL_SEQUENCE = /(\(\s*[a-dA-D]\s*\)|(^|[.:]\s+)[A-D]\s+[A-Z]).*(\(\s*[b-eB-E]\s*\)|[.:]\s+[B-E]\s+[A-Z])/s;

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&');

/**
 * How well a figure shows a step of an operation, or undefined when it does
 * not: a photo or drawing (not imaging, histology, an animal or a chart),
 * from an article or with a caption naming the procedure, whose caption
 * describes surgical work and mentions one of the step's hint words.
 */
export function stepFigureScore(
  image: Pick<MedicalImage, 'caption' | 'title' | 'modality' | 'panels'>,
  step: { names: string[]; hints: string[] }
): number | undefined {
  const caption = image.caption;
  const shows = [image.modality, ...(image.panels ?? [])];
  if (!shows.includes('photo') && !shows.includes('diagram')) return undefined;
  if (NOT_A_STEP.test(caption) || AFTERWARDS.test(caption) || !STEP_WORDS.test(caption)) return undefined;
  // "cesarean section" is named by "cesarean", in "cesarean scar" too
  const nameWords = step.names
    .flatMap((name) => name.toLowerCase().split(/[^a-z0-9]+/))
    .filter((word) => word.length > 2 && !GENERIC_NAME_WORDS.has(word));
  const text = `${image.title} ${caption}`.toLowerCase();
  if (!nameWords.some((word) => new RegExp(`\\b${escape(word)}`).test(text))) return undefined;
  const hints = step.hints.filter((hint) => new RegExp(`\\b${escape(hint.toLowerCase())}`).test(caption.toLowerCase()));
  if (hints.length === 0) return undefined;
  return hints.length + (PANEL_SEQUENCE.test(caption) ? 1 : 0) + (/\b(intra-?operative|step)/i.test(caption) ? 1 : 0);
}

/**
 * Up to `limit` openly licensed figures showing one step of a procedure,
 * best first: searched by the procedure and the step's own words, then by
 * the procedure's technique alone, and kept only if `stepFigureScore`
 * accepts them. On six procedure pages (how it is done, access, core steps,
 * closure), judged by hand, 18 of 27 figures found showed a step of that
 * procedure; half the steps found none.
 */
export async function findStepFigures(
  images: ImageSearchService,
  procedure: { title: string; names: string[] },
  step: { hints: string[]; topicHints?: string[] },
  limit = 3
): Promise<MedicalImage[]> {
  const own = step.topicHints && step.topicHints.length > 0 ? step.topicHints : step.hints;
  const hints = [...new Set([...step.hints, ...(step.topicHints ?? [])])];
  const queries = [`${procedure.title} ${own.slice(0, 2).join(' ')} technique`, `${procedure.title} surgical technique`];
  const found = new Map<string, { image: MedicalImage; score: number }>();
  for (const query of queries) {
    const { images: results } = await images.search(query, { type: 'photo', limit: 15 });
    for (const image of results) {
      const score = stepFigureScore(image, { names: procedure.names, hints });
      if (score !== undefined && !found.has(image.id)) found.set(image.id, { image, score });
    }
    if (found.size >= limit) break;
  }
  return [...found.values()]
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((f) => f.image);
}
