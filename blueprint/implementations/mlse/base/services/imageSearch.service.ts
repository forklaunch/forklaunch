import { collapseWhitespace, FetchLike, RateLimitedClient } from '../domain/http';
import { literatureSearchTerm } from './layTerms.service';
import { licenseScopeFor } from './licenseGate.service';
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
const MAX_FETCH = 100;

const ENTITIES: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' };

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
      timeoutMs: options.timeoutMs ?? 12_000
    });
    this.cacheMs = options.cacheMs ?? 60 * 60 * 1000;
    this.cacheEntries = options.cacheEntries ?? 500;
  }

  async search(query: string, options: { limit?: number; type?: ImageType } = {}): Promise<ImageSearchResult> {
    const text = query.trim();
    const limit = options.limit ?? 24;
    // only literature questions leave the service, as with live search
    const { queryClass } = classifyQuery(text);
    if (!text || (queryClass !== 'literature_lookup' && queryClass !== 'dosage_question')) {
      return { images: [], status: 'skipped' };
    }
    const key = `${options.type ?? ''}:${limit}:${text.toLowerCase()}`;
    const hit = this.cache.get(key);
    if (hit && Date.now() - hit.at < this.cacheMs) return { images: hit.value, status: 'ok' };

    // Open-i matches words: "heart attack" finds block diagrams, "myocardial
    // infarction" finds ECGs
    const params = new URLSearchParams({
      query: literatureSearchTerm(text),
      m: '1',
      n: String(Math.min(limit * FETCH_FACTOR, MAX_FETCH)),
      coll: 'pmc'
    });
    if (options.type) params.set('it', IMAGE_TYPES[options.type]);
    let items: OpenIItem[];
    try {
      const response = await this.client.getJson<{ list?: OpenIItem[] }>(`${this.baseUrl}/api/search?${params}`);
      items = Array.isArray(response.list) ? response.list : [];
    } catch {
      return { images: [], status: 'unavailable' };
    }

    const images = items
      .map((item) => this.toImage(item))
      .filter((image): image is MedicalImage => image !== undefined)
      .slice(0, limit);
    if (this.cache.size >= this.cacheEntries) {
      this.cache.delete(this.cache.keys().next().value as string);
    }
    this.cache.set(key, { at: Date.now(), value: images });
    return { images, status: 'ok' };
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
