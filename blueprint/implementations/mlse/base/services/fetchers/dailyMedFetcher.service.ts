import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  FetchedDocumentDto,
  SourceQueryDto
} from '@forklaunch/interfaces-mlse/types';
import { FetchLike, RateLimitedClient } from '../../domain/http';

type DailyMedResponse = {
  data?: {
    setid?: string;
    title?: string;
    published_date?: string;
    spl_version?: number;
  }[];
};

const MONTHS: Record<string, string> = {
  jan: '01', feb: '02', mar: '03', apr: '04', may: '05', jun: '06',
  jul: '07', aug: '08', sep: '09', oct: '10', nov: '11', dec: '12'
};

// DailyMed dates look like "Sep 18, 2026".
export function dailyMedDateToIso(value: string | undefined): string | undefined {
  const match = value?.match(/^([A-Za-z]{3})\w* (\d{1,2}), (\d{4})$/);
  if (!match) {
    return undefined;
  }
  const month = MONTHS[match[1].toLowerCase()];
  return month ? `${match[3]}-${month}-${match[2].padStart(2, '0')}` : undefined;
}

/**
 * The DailyMed label index (US government work). openFDA already supplies
 * each label's text, so DailyMed contributes the registry of label versions:
 * every current label for a drug with its version and date, linked to the
 * official DailyMed page. These documents carry no body text.
 */
export class DailyMedFetcher implements SourceFetcher {
  readonly sourceKey = 'dailymed';
  private readonly client: RateLimitedClient;
  private readonly baseUrl: string;

  constructor(fetchImpl: FetchLike, options: { baseUrl?: string } = {}) {
    this.client = new RateLimitedClient(this.sourceKey, fetchImpl, 250);
    this.baseUrl =
      options.baseUrl ?? 'https://dailymed.nlm.nih.gov/dailymed/services/v2/spls.json';
  }

  async fetchDocuments({
    term,
    limit
  }: SourceQueryDto): Promise<FetchedDocumentDto[]> {
    const url = `${this.baseUrl}?drug_name=${encodeURIComponent(term)}&pagesize=${Math.min(Math.max(limit, 1), 100)}`;
    const response = await this.client.getJson<DailyMedResponse>(url);
    return (response.data ?? [])
      .filter((spl) => spl.setid)
      .map((spl) => ({
        sourceKey: this.sourceKey,
        externalId: spl.setid as string,
        title: spl.title ?? (spl.setid as string),
        url: `https://dailymed.nlm.nih.gov/dailymed/drugInfo.cfm?setid=${spl.setid}`,
        publishedAt: dailyMedDateToIso(spl.published_date),
        license: 'us-government-work',
        sections: []
      }));
  }
}
