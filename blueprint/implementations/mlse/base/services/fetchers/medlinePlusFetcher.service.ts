import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  DocumentSectionDto,
  FetchedDocumentDto,
  SourceQueryDto
} from '@forklaunch/interfaces-mlse/types';
import {
  FetchLike,
  InProcessSchedule,
  RateLimitedClient,
  RequestOptions,
  RequestSchedule
} from '../../domain/http';
import { asArray, createXmlParser, textOf } from '../../domain/xml';

type MedlinePlusContent = { '@_name'?: string; '#text'?: string } | string;
type MedlinePlusDocument = { '@_url'?: string; content?: MedlinePlusContent | MedlinePlusContent[] };
type MedlinePlusResponse = {
  nlmSearchResult?: {
    list?: { document?: MedlinePlusDocument | MedlinePlusDocument[] };
    spellingCorrection?: string;
  };
};

export const MEDLINEPLUS_SEARCH_URL = 'https://wsearch.nlm.nih.gov/ws/query';
// the web service allows 85 requests a minute per address, across every
// caller, so the fetcher and spelling suggestions share one schedule
export const MEDLINEPLUS_INTERVAL_MS = 750;
const MEDLINEPLUS_SCHEDULE: RequestSchedule = new InProcessSchedule();

export function medlinePlusClient(fetchImpl: FetchLike): RateLimitedClient {
  return new RateLimitedClient('medlineplus', fetchImpl, MEDLINEPLUS_INTERVAL_MS, undefined, MEDLINEPLUS_SCHEDULE);
}

/**
 * MedlinePlus's spelling suggestion for a health topic search ("heart
 * atack" -> "heart attack"); undefined when it suggests nothing.
 */
export async function medlinePlusSpelling(
  client: RateLimitedClient,
  term: string,
  baseUrl = MEDLINEPLUS_SEARCH_URL,
  options: RequestOptions = {}
): Promise<string | undefined> {
  const url = `${baseUrl}?db=healthTopics&term=${encodeURIComponent(term)}&retmax=1`;
  const parsed = createXmlParser().parse(await client.getText(url, options)) as MedlinePlusResponse;
  const correction = textOf(String(parsed.nlmSearchResult?.spellingCorrection ?? '')).trim();
  return correction || undefined;
}

// NLM asks for this credit on reused public-domain MedlinePlus content.
export const MEDLINEPLUS_CREDIT = 'MedlinePlus, National Library of Medicine';

const MAX_HEADING_LENGTH = 140;

function contentNamed(doc: MedlinePlusDocument, name: string): string {
  const match = asArray(doc.content).find((c) => typeof c === 'object' && c['@_name'] === name);
  return match && typeof match === 'object' ? String(match['#text'] ?? '') : '';
}

/**
 * Splits a health topic summary into its question-headed sections ("What
 * are the symptoms of a heart attack?"). The summary is HTML: headings are
 * plain text, answers are paragraphs and lists. A plain scan over the tag
 * boundaries, so linear on any input.
 */
export function summarySections(html: string): DocumentSectionDto[] {
  const lines = html
    .replaceAll('</p>', '\n')
    .replaceAll('<p>', '\n')
    .replaceAll('</li>', '\n')
    .replaceAll('<li>', '\n')
    .replaceAll('<ul>', '\n')
    .replaceAll('</ul>', '\n')
    .split('\n')
    // highlighting markup leaves a space before punctuation: "attack ?"
    .map((line) => textOf(line).replaceAll(' ?', '?').replaceAll(' !', '!'))
    .filter((line) => line.length > 0);

  const sections: DocumentSectionDto[] = [];
  let heading = 'Summary';
  let body: string[] = [];
  const flush = () => {
    if (body.length > 0) sections.push({ path: heading, text: body.join(' ') });
    body = [];
  };
  for (const line of lines) {
    if (line.endsWith('?') && line.length <= MAX_HEADING_LENGTH) {
      flush();
      heading = line;
    } else {
      body.push(line);
    }
  }
  flush();
  return sections;
}

/**
 * MedlinePlus health topics from the US National Library of Medicine: plain
 * summaries of conditions, their symptoms, causes and treatment, which
 * research abstracts rarely state. Only the NLM-written health topic
 * summaries are used; they are public domain. Other MedlinePlus content
 * (the A.D.A.M. encyclopedia, drug monographs) is copyrighted and is not
 * fetched.
 */
export class MedlinePlusFetcher implements SourceFetcher {
  readonly sourceKey = 'medlineplus';
  private readonly client: RateLimitedClient;

  constructor(
    fetchImpl: FetchLike,
    private readonly baseUrl = MEDLINEPLUS_SEARCH_URL
  ) {
    this.client = medlinePlusClient(fetchImpl);
  }

  async fetchDocuments(
    { term, limit }: SourceQueryDto,
    { signal }: { signal?: AbortSignal } = {}
  ): Promise<FetchedDocumentDto[]> {
    const url = `${this.baseUrl}?db=healthTopics&term=${encodeURIComponent(term)}&retmax=${Math.min(Math.max(limit, 1), 20)}`;
    const parsed = createXmlParser().parse(
      await this.client.getText(url, { signal })
    ) as MedlinePlusResponse;
    return asArray(parsed.nlmSearchResult?.list?.document)
      .map((doc) => this.toDocument(doc))
      .filter((doc): doc is FetchedDocumentDto => doc !== undefined);
  }

  private toDocument(doc: MedlinePlusDocument): FetchedDocumentDto | undefined {
    const url = doc['@_url'];
    const title = textOf(contentNamed(doc, 'title'));
    const sections = summarySections(contentNamed(doc, 'FullSummary'));
    if (!url || !url.startsWith('https://medlineplus.gov/') || !title || sections.length === 0) {
      return undefined;
    }
    const slug = url.slice('https://medlineplus.gov/'.length).replace('.html', '');
    return {
      sourceKey: this.sourceKey,
      externalId: slug,
      title: `${title} (${MEDLINEPLUS_CREDIT})`,
      url,
      license: 'us-government-work',
      sections
    };
  }
}
