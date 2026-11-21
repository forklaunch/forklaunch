import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import { LicensedContentAdapter } from './licensedContentAdapter.service';

/**
 * The fetchers MLSE can refresh from, looked up by source key. A class rather
 * than a plain map so it can be registered in the dependency container like
 * any other service.
 */
export class SourceFetcherRegistry {
  private readonly fetchers: Map<string, SourceFetcher>;

  constructor(fetchers: SourceFetcher[]) {
    this.fetchers = new Map();
    for (const fetcher of fetchers) {
      if (this.fetchers.has(fetcher.sourceKey)) {
        throw new Error(`Duplicate fetcher for source '${fetcher.sourceKey}'`);
      }
      this.fetchers.set(fetcher.sourceKey, fetcher);
    }
  }

  get(sourceKey: string): SourceFetcher | undefined {
    return this.fetchers.get(sourceKey);
  }

  has(sourceKey: string): boolean {
    return this.fetchers.has(sourceKey);
  }

  // true for licensed publishers wrapped in LicensedContentAdapter
  isLicensed(sourceKey: string): boolean {
    return this.fetchers.get(sourceKey) instanceof LicensedContentAdapter;
  }

  keys(): string[] {
    return [...this.fetchers.keys()];
  }
}
