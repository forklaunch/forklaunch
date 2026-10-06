import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  FetchedDocumentDto,
  SourceQueryDto
} from '@forklaunch/interfaces-mlse/types';

// License strings the license gate maps for content used under a contract
// rather than an open license.
export const LICENSED_FULL_TEXT = 'licensed-full-text';
export const LICENSED_EXCERPT = 'licensed-excerpt';

export type LicensedContentTerms = {
  // what the contract allows MLSE to store and show
  scope: 'full_text' | 'excerpt_only';
};

/**
 * Wraps a fetcher for a licensed publisher (a subscription database, a
 * society's guideline feed) so its documents are stored under the contract's
 * terms instead of whatever license string the feed carries.
 *
 * A client developer writes the publisher-specific fetcher, wraps it here and
 * adds it to the fetcher registry, then registers the source with
 * requiresLicense. Its content is only searchable for organizations with an
 * active content license, and ingestion refuses it while no license is
 * active.
 */
export class LicensedContentAdapter implements SourceFetcher {
  readonly sourceKey: string;

  constructor(
    private readonly fetcher: SourceFetcher,
    private readonly terms: LicensedContentTerms
  ) {
    this.sourceKey = fetcher.sourceKey;
  }

  async fetchDocuments(query: SourceQueryDto): Promise<FetchedDocumentDto[]> {
    const license =
      this.terms.scope === 'full_text' ? LICENSED_FULL_TEXT : LICENSED_EXCERPT;
    const documents = await this.fetcher.fetchDocuments(query);
    return documents.map((document) => ({ ...document, license }));
  }
}
