import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  DocumentSectionDto,
  FetchedDocumentDto,
  SourceQueryDto
} from '@forklaunch/interfaces-mlse/types';
import {
  collapseWhitespace,
  FetchLike,
  RateLimitedClient
} from '../../domain/http';

type CtGovStudy = {
  protocolSection?: {
    identificationModule?: { nctId?: string; briefTitle?: string; officialTitle?: string };
    statusModule?: {
      overallStatus?: string;
      lastUpdatePostDateStruct?: { date?: string };
    };
    descriptionModule?: { briefSummary?: string; detailedDescription?: string };
    conditionsModule?: { conditions?: string[] };
    designModule?: { studyType?: string; phases?: string[] };
    armsInterventionsModule?: {
      interventions?: { type?: string; name?: string; description?: string }[];
    };
    eligibilityModule?: { eligibilityCriteria?: string; minimumAge?: string; maximumAge?: string; sex?: string };
    outcomesModule?: {
      primaryOutcomes?: { measure?: string; timeFrame?: string }[];
    };
  };
  hasResults?: boolean;
};

type CtGovResponse = { studies?: CtGovStudy[] };

/**
 * Registered trials from ClinicalTrials.gov (API v2, public registry data).
 * The overall status is kept in the text so an answer can say whether a
 * trial is recruiting, completed, or has posted results.
 */
export class ClinicalTrialsFetcher implements SourceFetcher {
  readonly sourceKey = 'clinicaltrials';
  private readonly client: RateLimitedClient;
  private readonly baseUrl: string;

  constructor(fetchImpl: FetchLike, options: { baseUrl?: string } = {}) {
    this.client = new RateLimitedClient(this.sourceKey, fetchImpl, 250);
    this.baseUrl = options.baseUrl ?? 'https://clinicaltrials.gov/api/v2/studies';
  }

  async fetchDocuments({
    term,
    limit
  }: SourceQueryDto): Promise<FetchedDocumentDto[]> {
    const url = `${this.baseUrl}?query.term=${encodeURIComponent(term)}&pageSize=${Math.min(Math.max(limit, 1), 100)}&format=json`;
    const response = await this.client.getJson<CtGovResponse>(url);
    return (response.studies ?? [])
      .map((study) => this.toDocument(study))
      .filter((doc): doc is FetchedDocumentDto => doc !== undefined);
  }

  toDocument(study: CtGovStudy): FetchedDocumentDto | undefined {
    const protocol = study.protocolSection;
    const nctId = protocol?.identificationModule?.nctId;
    if (!protocol || !nctId) {
      return undefined;
    }

    const sections: DocumentSectionDto[] = [];
    const push = (path: string, text: string | undefined) => {
      const cleaned = text ? collapseWhitespace(text) : '';
      if (cleaned) {
        sections.push({ path, text: cleaned });
      }
    };

    const status = protocol.statusModule?.overallStatus;
    const design = protocol.designModule;
    push(
      'Status',
      [
        status ? `Overall status: ${status.replace(/_/g, ' ').toLowerCase()}.` : undefined,
        design?.studyType ? `Study type: ${design.studyType.toLowerCase()}.` : undefined,
        design?.phases?.length ? `Phase: ${design.phases.join(', ')}.` : undefined,
        study.hasResults ? 'Results posted.' : 'No results posted.'
      ]
        .filter(Boolean)
        .join(' ')
    );
    push('Conditions', protocol.conditionsModule?.conditions?.join(', '));
    push('Brief Summary', protocol.descriptionModule?.briefSummary);
    push('Detailed Description', protocol.descriptionModule?.detailedDescription);
    push(
      'Interventions',
      protocol.armsInterventionsModule?.interventions
        ?.map((i) =>
          [i.type ? `${i.type.toLowerCase()}:` : undefined, i.name, i.description ? `— ${i.description}` : undefined]
            .filter(Boolean)
            .join(' ')
        )
        .join('; ')
    );
    push(
      'Primary Outcomes',
      protocol.outcomesModule?.primaryOutcomes
        ?.map((o) => [o.measure, o.timeFrame ? `(${o.timeFrame})` : undefined].filter(Boolean).join(' '))
        .join('; ')
    );
    push('Eligibility', protocol.eligibilityModule?.eligibilityCriteria);

    return {
      sourceKey: this.sourceKey,
      externalId: nctId,
      title:
        protocol.identificationModule?.briefTitle ??
        protocol.identificationModule?.officialTitle ??
        nctId,
      url: `https://clinicaltrials.gov/study/${nctId}`,
      publishedAt: protocol.statusModule?.lastUpdatePostDateStruct?.date,
      license: 'public-domain',
      sections
    };
  }
}
