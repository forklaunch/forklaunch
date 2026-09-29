import { FetchLike, RateLimitedClient, RequestSchedule } from '../../domain/http';

// NCBI's limit is per application (API key, or address without one), across
// every E-utilities database, so PubMed and PubMed Central share one schedule.
const NCBI_SCHEDULES = new Map<string, RequestSchedule>();

export type EutilsOptions = {
  // NCBI asks every application to identify itself with a tool name and a
  // contact email, and to let each deployment use its own API key.
  tool: string;
  email: string;
  apiKey?: string;
  baseUrl?: string;
};

type EsearchResponse = {
  esearchresult?: { idlist?: string[] };
};

/**
 * Thin client for NCBI E-utilities (esearch + efetch), shared by the PubMed
 * and PubMed Central fetchers. Requests are spaced to NCBI's limits: 3 per
 * second without an API key, 10 with one.
 */
export class EutilsClient {
  private readonly client: RateLimitedClient;
  private readonly baseUrl: string;

  constructor(
    sourceKey: string,
    fetchImpl: FetchLike,
    private readonly options: EutilsOptions
  ) {
    this.baseUrl =
      options.baseUrl ?? 'https://eutils.ncbi.nlm.nih.gov/entrez/eutils';
    const scheduleKey = `${this.baseUrl}|${options.apiKey ?? ''}`;
    const schedule = NCBI_SCHEDULES.get(scheduleKey) ?? { nextSlot: 0 };
    NCBI_SCHEDULES.set(scheduleKey, schedule);
    this.client = new RateLimitedClient(
      sourceKey,
      fetchImpl,
      options.apiKey ? 110 : 350,
      undefined,
      schedule
    );
  }

  private identity(): string {
    const params = [
      `tool=${encodeURIComponent(this.options.tool)}`,
      `email=${encodeURIComponent(this.options.email)}`
    ];
    if (this.options.apiKey) {
      params.push(`api_key=${encodeURIComponent(this.options.apiKey)}`);
    }
    return params.join('&');
  }

  async search(db: 'pubmed' | 'pmc', term: string, limit: number): Promise<string[]> {
    // PubMed ranks by Best Match only when asked; otherwise the newest
    // papers come first, which answers a general question with niche work
    const sort = db === 'pubmed' ? '&sort=relevance' : '';
    const url = `${this.baseUrl}/esearch.fcgi?db=${db}&term=${encodeURIComponent(term)}&retmax=${Math.min(Math.max(limit, 1), 200)}${sort}&retmode=json&${this.identity()}`;
    const response = await this.client.getJson<EsearchResponse>(url);
    return response.esearchresult?.idlist ?? [];
  }

  async fetchXml(db: 'pubmed' | 'pmc', ids: string[]): Promise<string> {
    const url = `${this.baseUrl}/efetch.fcgi?db=${db}&id=${ids.map(encodeURIComponent).join(',')}&retmode=xml&${this.identity()}`;
    return this.client.getText(url);
  }
}
