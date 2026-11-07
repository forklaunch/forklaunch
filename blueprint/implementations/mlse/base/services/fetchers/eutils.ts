import {
  FetchLike,
  InProcessSchedule,
  RateLimitedClient,
  RequestOptions,
  RequestSchedule
} from '../../domain/http';

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
  // NCBI's limit covers every process using the same key; a deployment
  // running more than one passes a schedule they share (see mlse-base)
  schedule?: RequestSchedule;
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
    const schedule =
      options.schedule ??
      NCBI_SCHEDULES.get(scheduleKey) ??
      new InProcessSchedule();
    if (!options.schedule) NCBI_SCHEDULES.set(scheduleKey, schedule);
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

  async search(
    db: 'pubmed' | 'pmc',
    term: string,
    limit: number,
    options: RequestOptions = {}
  ): Promise<string[]> {
    // Both rank by relevance only when asked; otherwise the newest papers
    // come first, which answers a general question with niche work (PMC's
    // newest "cesarean section" papers include microbiome modelling)
    const url = `${this.baseUrl}/esearch.fcgi?db=${db}&term=${encodeURIComponent(term)}&retmax=${Math.min(Math.max(limit, 1), 200)}&sort=relevance&retmode=json&${this.identity()}`;
    const response = await this.client.getJson<EsearchResponse>(url, options);
    return response.esearchresult?.idlist ?? [];
  }

  // NCBI's spelling suggestion for a PubMed query ("myocardail infraction"
  // -> "myocardial infarction"); undefined when it suggests nothing
  async spell(
    term: string,
    options: RequestOptions = {}
  ): Promise<string | undefined> {
    const url = `${this.baseUrl}/espell.fcgi?db=pubmed&term=${encodeURIComponent(term)}&${this.identity()}`;
    const xml = await this.client.getText(url, options);
    const start = xml.indexOf('<CorrectedQuery>');
    const end = xml.indexOf('</CorrectedQuery>');
    if (start < 0 || end < start) return undefined;
    const corrected = xml.slice(start + '<CorrectedQuery>'.length, end).trim();
    return corrected || undefined;
  }

  async fetchXml(
    db: 'pubmed' | 'pmc',
    ids: string[],
    options: RequestOptions = {}
  ): Promise<string> {
    const url = `${this.baseUrl}/efetch.fcgi?db=${db}&id=${ids.map(encodeURIComponent).join(',')}&retmode=xml&${this.identity()}`;
    return this.client.getText(url, options);
  }
}
