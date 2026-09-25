import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  DocumentSectionDto,
  FetchedDocumentDto,
  SourceQueryDto
} from '@forklaunch/interfaces-mlse/types';
import {
  collapseWhitespace,
  compactDateToIso,
  FetchLike,
  RateLimitedClient
} from '../../domain/http';

// Label sections MLSE uses, in reading order, with the heading shown to
// doctors. openFDA returns each as an array of strings.
const LABEL_SECTIONS: [field: string, heading: string][] = [
  ['boxed_warning', 'Boxed Warning'],
  ['indications_and_usage', 'Indications and Usage'],
  ['dosage_and_administration', 'Dosage and Administration'],
  ['contraindications', 'Contraindications'],
  ['warnings_and_cautions', 'Warnings and Precautions'],
  ['warnings', 'Warnings'],
  ['precautions', 'Precautions'],
  ['adverse_reactions', 'Adverse Reactions'],
  ['drug_interactions', 'Drug Interactions'],
  ['use_in_specific_populations', 'Use in Specific Populations'],
  ['pregnancy', 'Pregnancy'],
  ['pediatric_use', 'Pediatric Use'],
  ['geriatric_use', 'Geriatric Use'],
  ['overdosage', 'Overdosage']
];

type OpenFdaLabel = {
  set_id?: string;
  id?: string;
  effective_time?: string;
  openfda?: {
    brand_name?: string[];
    generic_name?: string[];
    manufacturer_name?: string[];
  };
} & Record<string, unknown>;

type OpenFdaResponse = { results?: OpenFdaLabel[] };

export type OpenFdaFetcherOptions = {
  apiKey?: string;
  baseUrl?: string;
};

/**
 * Drug labels from openFDA (public domain, CC0). Searches generic and brand
 * names and returns one document per label set, sectioned the way the label
 * itself is.
 */
export class OpenFdaFetcher implements SourceFetcher {
  readonly sourceKey = 'openfda';
  private readonly client: RateLimitedClient;
  private readonly apiKey?: string;
  private readonly baseUrl: string;

  constructor(fetchImpl: FetchLike, options: OpenFdaFetcherOptions = {}) {
    // openFDA allows 240 requests/minute; stay under it.
    this.client = new RateLimitedClient(this.sourceKey, fetchImpl, 260);
    this.apiKey = options.apiKey || undefined;
    this.baseUrl = options.baseUrl ?? 'https://api.fda.gov/drug/label.json';
  }

  async fetchDocuments({
    term,
    limit
  }: SourceQueryDto): Promise<FetchedDocumentDto[]> {
    const quoted = `"${term.replace(/"/g, '')}"`;
    const search = `openfda.generic_name:${quoted}+openfda.brand_name:${quoted}`;
    const params = [
      `search=${encodeURIComponent(search).replace(/%2B/g, '+')}`,
      `limit=${Math.min(Math.max(limit, 1), 100)}`
    ];
    if (this.apiKey) {
      params.push(`api_key=${encodeURIComponent(this.apiKey)}`);
    }

    let response: OpenFdaResponse;
    try {
      response = await this.client.getJson<OpenFdaResponse>(
        `${this.baseUrl}?${params.join('&')}`
      );
    } catch (error) {
      // openFDA answers a search with no matches with HTTP 404.
      if (error instanceof Error && 'status' in error && error.status === 404) {
        return [];
      }
      throw error;
    }

    return (response.results ?? [])
      .map((label) => this.toDocument(label))
      .filter((doc): doc is FetchedDocumentDto => doc !== undefined);
  }

  toDocument(label: OpenFdaLabel): FetchedDocumentDto | undefined {
    const setId = label.set_id;
    if (!setId) {
      return undefined;
    }

    const sections: DocumentSectionDto[] = [];
    for (const [field, heading] of LABEL_SECTIONS) {
      const value = label[field];
      if (Array.isArray(value)) {
        const text = collapseWhitespace(value.join(' '));
        if (text) {
          sections.push({ path: heading, text });
        }
      }
    }

    const generic = label.openfda?.generic_name?.[0];
    const brand = label.openfda?.brand_name?.[0];
    const manufacturer = label.openfda?.manufacturer_name?.[0];
    const name = [brand ?? generic, generic && brand && generic !== brand.toUpperCase() ? `(${generic.toLowerCase()})` : undefined]
      .filter(Boolean)
      .join(' ');

    return {
      sourceKey: this.sourceKey,
      externalId: setId,
      title: [name || 'Drug label', manufacturer ? `— ${manufacturer}` : undefined]
        .filter(Boolean)
        .join(' '),
      url: `https://dailymed.nlm.nih.gov/dailymed/lookup.cfm?setid=${setId}`,
      publishedAt: compactDateToIso(label.effective_time),
      license: 'CC0',
      sections
    };
  }
}
