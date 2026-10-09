import { collapseWhitespace, FetchLike, RateLimitedClient, SourceTimeoutError } from '../domain/http';
import { literatureSearchTerm } from './layTerms.service';
import { licenseScopeFor } from './licenseGate.service';
import { queryTerms } from './ranking.service';
import { classifyQuery } from './queryClassifier.service';

// Open-i's image types, by the name a caller filters on
export const IMAGE_TYPES = {
  photo: 'ph',
  xray: 'x',
  ct: 'c',
  mri: 'm',
  ultrasound: 'u',
  microscopy: 'mc',
  diagram: 'g'
} as const;

export type ImageType = keyof typeof IMAGE_TYPES;

const MODALITY_NAMES: Record<string, string> = {
  ph: 'photo',
  x: 'x-ray',
  c: 'CT',
  m: 'MRI',
  u: 'ultrasound',
  mc: 'microscopy',
  p: 'PET',
  g: 'diagram',
  mp: 'multi-panel figure'
};

export type MedicalImage = {
  id: string;
  caption: string;
  thumbnailUrl: string;
  imageUrl: string;
  // the article the figure comes from, which is cited with it
  title: string;
  articleUrl: string;
  pmcid: string;
  journal?: string;
  year?: string;
  authors?: string;
  modality?: string;
  // what the panels of a multi-panel figure show
  panels?: string[];
  license: string;
  licenseUrl: string;
};

export type ImageSearchResult = {
  images: MedicalImage[];
  // 'skipped' when the query describes a patient and was not sent out
  status: 'ok' | 'skipped' | 'unavailable';
};

export type ImageSearchOptions = {
  baseUrl?: string;
  timeoutMs?: number;
  cacheMs?: number;
  cacheEntries?: number;
};

type OpenIItem = {
  uid?: string;
  pmcid?: string;
  title?: string;
  authors?: string;
  journal_title?: string;
  journal_date?: { year?: string | null };
  pmc_url?: string;
  ccLicense?: string;
  licenseURL?: string;
  imgLarge?: string;
  // the 137 px size Open-i lists is not served; the 150 px grid size is
  imgGrid150?: string;
  image?: { id?: string; caption?: string; modalityMajor?: string; modalityMinor?: string };
};

// About a third of Open-i's figures carry a license that allows reuse, so a
// page asks for more than it shows.
const FETCH_FACTOR = 4;
const MAX_FETCH = 60;
// Open-i sends each figure's abstract too and sends slowly: 60 figures took
// 3 to 14 s, so they are asked for in pages fetched together
const PAGE_SIZE = 30;

const ENTITIES: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' };

// "infarct" and "infarction", "appendix" and "appendices" count as one word
function sameWord(a: string, b: string): boolean {
  if (a === b) return true;
  const [short, long] = a.length < b.length ? [a, b] : [b, a];
  return short.length >= 5 && long.startsWith(short.slice(0, Math.max(5, short.length - 2)));
}

function share(terms: string[], text: string): number {
  if (terms.length === 0) return 0;
  const words = queryTerms(text);
  return terms.filter((term) => words.some((word) => sameWord(term, word))).length / terms.length;
}

/**
 * Open-i matches a query anywhere in the article, abstract included, so a
 * figure of hippocampal staining comes back for "appendectomy" from a paper
 * with appendectomy in its title. A figure is kept when its caption carries
 * at least half the query's words; with `orTitle`, when its caption and
 * article title together do, those whose caption names them first.
 */
export function rankByCaption<T extends { caption: string; title: string }>(
  images: T[],
  query: string,
  options: { orTitle?: boolean } = {}
): T[] {
  const terms = queryTerms(query);
  return images
    .map((image, order) => ({
      image,
      order,
      caption: share(terms, image.caption),
      either: share(terms, `${image.caption} ${image.title}`)
    }))
    .filter((r) => terms.length === 0 || (options.orTitle ? r.either : r.caption) >= 0.5)
    .sort((a, b) => b.caption - a.caption || a.order - b.order)
    .map((r) => r.image);
}

function plainText(html: string | undefined): string {
  return collapseWhitespace(
    (html ?? '')
      .replace(/<[^>]{0,200}>/g, '')
      .replace(/&(#\d{1,7}|[a-z]{2,6});/gi, (entity, name: string) =>
        name.startsWith('#') ? String.fromCodePoint(Number(name.slice(1))) : (ENTITIES[name.toLowerCase()] ?? entity)
      )
  );
}

// "http://creativecommons.org/licenses/by-sa/3.0/" -> "CC BY-SA 3.0"
function licenseLabel(url: string): string {
  const [kind, version] = url.replace(/^https?:\/\/creativecommons\.org\/(licenses|publicdomain)\//, '').split('/');
  if (kind === 'mark') return 'Public domain';
  return kind === 'zero' ? 'CC0' : `CC ${kind.toUpperCase()}${version ? ` ${version}` : ''}`;
}

/**
 * Figures from open-access articles, like a web search's image results, from
 * NLM's Open-i. Only figures whose license allows reuse in a commercial
 * product are returned (the rule the full text follows), each with the
 * article to cite and its license.
 */
export class ImageSearchService {
  private readonly client: RateLimitedClient;
  private readonly baseUrl: string;
  private readonly cache = new Map<string, { at: number; value: MedicalImage[] }>();
  private readonly cacheMs: number;
  private readonly cacheEntries: number;

  constructor(fetchImpl: FetchLike, options: ImageSearchOptions = {}) {
    this.baseUrl = options.baseUrl ?? 'https://openi.nlm.nih.gov';
    this.client = new RateLimitedClient('openi', fetchImpl, 200, undefined, undefined, {
      timeoutMs: options.timeoutMs ?? 15_000
    });
    this.cacheMs = options.cacheMs ?? 60 * 60 * 1000;
    this.cacheEntries = options.cacheEntries ?? 500;
  }

  // `about`: the words a figure must be about, when the query adds others to
  // help the search find it ("appendectomy" for "appendectomy surgical technique")
  // `orTitle`: also keep figures whose article title, not caption, names it
  async search(
    query: string,
    options: { limit?: number; type?: ImageType; about?: string; orTitle?: boolean } = {}
  ): Promise<ImageSearchResult> {
    const text = query.trim();
    const limit = options.limit ?? 24;
    // only literature questions leave the service, as with live search
    const { queryClass } = classifyQuery(text);
    if (!text || (queryClass !== 'literature_lookup' && queryClass !== 'dosage_question')) {
      return { images: [], status: 'skipped' };
    }
    const key = `${options.type ?? ''}:${limit}:${options.about ?? ''}:${options.orTitle ? 't' : ''}:${text.toLowerCase()}`;
    const hit = this.cache.get(key);
    if (hit && Date.now() - hit.at < this.cacheMs) return { images: hit.value, status: 'ok' };

    // Open-i matches words: "heart attack" finds block diagrams, "myocardial
    // infarction" finds ECGs
    const term = literatureSearchTerm(text);
    const wanted = Math.min(limit * FETCH_FACTOR, MAX_FETCH);
    const pages: Promise<OpenIItem[] | undefined>[] = [];
    for (let first = 1; first <= wanted; first += PAGE_SIZE) {
      const params = new URLSearchParams({
        query: term,
        m: String(first),
        n: String(Math.min(first + PAGE_SIZE - 1, wanted)),
        coll: 'pmc'
      });
      if (options.type) params.set('it', IMAGE_TYPES[options.type]);
      pages.push(this.page(params));
    }
    const answered = (await Promise.all(pages)).filter((page): page is OpenIItem[] => page !== undefined);
    if (answered.length === 0) return { images: [], status: 'unavailable' };

    const found = new Map<string, MedicalImage>();
    for (const item of answered.flat()) {
      const image = this.toImage(item);
      if (image && !found.has(image.id)) found.set(image.id, image);
    }
    const images = rankByCaption(
      [...found.values()],
      options.about ? literatureSearchTerm(options.about) : term,
      { orTitle: options.orTitle }
    ).slice(0, limit);
    if (this.cache.size >= this.cacheEntries) {
      this.cache.delete(this.cache.keys().next().value as string);
    }
    this.cache.set(key, { at: Date.now(), value: images });
    return { images, status: 'ok' };
  }

  // One page of results, or undefined when Open-i did not answer. It now and
  // then fails a request it answers a moment later, so a failed one is asked
  // again once; one that timed out is not.
  private async page(params: URLSearchParams): Promise<OpenIItem[] | undefined> {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        const response = await this.client.getJson<{ list?: OpenIItem[] }>(`${this.baseUrl}/api/search?${params}`);
        return Array.isArray(response.list) ? response.list : [];
      } catch (error) {
        if (error instanceof SourceTimeoutError) return undefined;
      }
    }
    return undefined;
  }

  private toImage(item: OpenIItem): MedicalImage | undefined {
    const licenseUrl = item.licenseURL?.trim();
    if (!licenseUrl || licenseScopeFor(licenseUrl) !== 'full_text') return undefined;
    if (!item.imgLarge || !item.pmcid || !/^\d+$/.test(item.pmcid)) return undefined;
    const pmcid = `PMC${item.pmcid}`;
    const modality = item.image?.modalityMajor;
    const panels = (item.image?.modalityMinor ?? '')
      .split(',')
      .map((code) => MODALITY_NAMES[code.trim()])
      .filter(Boolean);
    return {
      id: `${item.uid ?? pmcid}:${item.image?.id ?? item.imgLarge}`,
      caption: plainText(item.image?.caption),
      thumbnailUrl: `${this.baseUrl}${item.imgGrid150 ?? item.imgLarge}`,
      imageUrl: `${this.baseUrl}${item.imgLarge}`,
      title: plainText(item.title),
      articleUrl: `https://pmc.ncbi.nlm.nih.gov/articles/${pmcid}/`,
      pmcid,
      ...(item.journal_title ? { journal: plainText(item.journal_title) } : {}),
      ...(item.journal_date?.year ? { year: item.journal_date.year } : {}),
      ...(item.authors ? { authors: item.authors } : {}),
      ...(modality && MODALITY_NAMES[modality] ? { modality: MODALITY_NAMES[modality] } : {}),
      ...(panels.length > 0 ? { panels } : {}),
      license: licenseLabel(licenseUrl),
      licenseUrl
    };
  }
}
