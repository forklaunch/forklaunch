import { FetchLike, RateLimitedClient } from '../../domain/http';

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
    this.client = new RateLimitedClient(
      sourceKey,
      fetchImpl,
      options.apiKey ? 110 : 350
    );
    this.baseUrl =
      options.baseUrl ?? 'https://eutils.ncbi.nlm.nih.gov/entrez/eutils';
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
    const url = `${this.baseUrl}/esearch.fcgi?db=${db}&term=${encodeURIComponent(term)}&retmax=${Math.min(Math.max(limit, 1), 200)}&retmode=json&${this.identity()}`;
    const response = await this.client.getJson<EsearchResponse>(url);
    return response.esearchresult?.idlist ?? [];
  }

  async fetchXml(db: 'pubmed' | 'pmc', ids: string[]): Promise<string> {
    const url = `${this.baseUrl}/efetch.fcgi?db=${db}&id=${ids.map(encodeURIComponent).join(',')}&retmode=xml&${this.identity()}`;
    return this.client.getText(url);
  }
}
