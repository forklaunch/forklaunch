import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import { FetchedDocumentDto, SourceQueryDto } from '@forklaunch/interfaces-mlse/types';
import { FetchLike } from '../../domain/http';
import { licenseScopeFor } from '../licenseGate.service';
import { EutilsOptions } from './eutils';
import { PmcOaFetcher } from './pmcOaFetcher.service';
import { PubMedFetcher } from './pubmedFetcher.service';

export type GuidelineFetcherOptions = {
  // guidelines older than this are left out: they are revised, and an old
  // one can contradict its replacement
  sinceYears?: number;
  now?: () => Date;
};

/**
 * Clinical practice guidelines and consensus statements: PubMed records
 * indexed as guidelines, from the last ten years by default. A guideline
 * whose PubMed Central copy carries a license that allows commercial reuse
 * is returned in full; the rest, which includes most society guidelines
 * (published under the society's copyright), as their PubMed abstract,
 * which MLSE shows as a short excerpt with a link.
 */
export class GuidelineFetcher implements SourceFetcher {
  readonly sourceKey = 'guidelines';
  private readonly pubmed: PubMedFetcher;
  private readonly pmc: PmcOaFetcher;
  private readonly sinceYears: number;
  private readonly now: () => Date;

  constructor(fetchImpl: FetchLike, ncbi: EutilsOptions, options: GuidelineFetcherOptions = {}) {
    this.pubmed = new PubMedFetcher(fetchImpl, ncbi);
    this.pmc = new PmcOaFetcher(fetchImpl, ncbi);
    this.sinceYears = options.sinceYears ?? 10;
    this.now = options.now ?? (() => new Date());
  }

  async fetchDocuments(
    { term, limit }: SourceQueryDto,
    { signal }: { signal?: AbortSignal } = {}
  ): Promise<FetchedDocumentDto[]> {
    const since = this.now().getUTCFullYear() - this.sinceYears;
    const records = await this.pubmed.fetchDocuments(
      { term: `(${term}) AND (guideline[pt] OR practice guideline[pt]) AND ${since}:3000[dp]`, limit },
      { signal }
    );
    const pmcids = records.map((record) => record.pmcid).filter((id): id is string => id !== undefined);
    const fullText = new Map(
      (pmcids.length > 0 ? await this.pmc.fetchOpenAccess(pmcids, { signal }) : [])
        .filter((article) => licenseScopeFor(article.license) === 'full_text')
        .map((article) => [article.externalId, article])
    );

    // a record without an abstract (common for older society guidelines)
    // has nothing to cite
    return records.filter((record) => record.sections.length > 0 || fullText.has(record.pmcid ?? '')).map((record) => {
      const article = record.pmcid ? fullText.get(record.pmcid) : undefined;
      if (!article) {
        return { ...record, sourceKey: this.sourceKey };
      }
      return {
        ...article,
        sourceKey: this.sourceKey,
        // PubMed's indexing is the better record of what the guideline is about
        publishedAt: record.publishedAt ?? article.publishedAt,
        retracted: record.retracted || article.retracted,
        meshDescriptorUis: record.meshDescriptorUis
      };
    });
  }
}
