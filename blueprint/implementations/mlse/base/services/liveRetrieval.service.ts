import { createHash } from 'node:crypto';
import {
  CitablePassageDto,
  FetchedDocumentDto,
  LiveSourceResultDto
} from '@forklaunch/interfaces-mlse/types';
import { chunkSections } from './chunker.service';
import { licenseScopeFor } from './licenseGate.service';
import { applyLicense } from './licenseText.service';
import { SourceFetcherRegistry } from './sourceFetcherRegistry.service';

// The subset of the framework's TtlCache that live retrieval uses.
export type LiveResultCache = {
  peekRecord(key: string): Promise<boolean>;
  readRecord<T>(key: string): Promise<{ value: T }>;
  putRecord<T>(record: { key: string; value: T; ttlMilliseconds: number }): Promise<void>;
};

export type LiveRetrievalOptions = {
  // budget for all live sources together, inside the 10-second answer target
  timeoutMs?: number;
  documentsPerSource?: number;
  cacheTtlMs?: number;
  excerptChars?: number;
  maxPassageChars?: number;
};

export type LiveRetrievalResult = {
  passages: CitablePassageDto[];
  sources: LiveSourceResultDto[];
};

class TimeoutError extends Error {}

/**
 * Queries medical sources at search time so answers include publications
 * newer than the last corpus refresh. Every document passes the same license
 * gate as ingestion; retracted documents are dropped. Sources are queried in
 * parallel under one time budget, and a slow or failing source is reported
 * rather than failing the search. Responses are cached per source and term.
 */
export class LiveRetrievalService {
  private readonly timeoutMs: number;
  private readonly documentsPerSource: number;
  private readonly cacheTtlMs: number;
  private readonly excerptChars: number;
  private readonly maxPassageChars: number;

  constructor(
    private readonly fetchers: SourceFetcherRegistry,
    private readonly liveSourceKeys: string[],
    private readonly cache?: LiveResultCache,
    options: LiveRetrievalOptions = {}
  ) {
    this.timeoutMs = options.timeoutMs ?? 4000;
    this.documentsPerSource = options.documentsPerSource ?? 5;
    this.cacheTtlMs = options.cacheTtlMs ?? 6 * 60 * 60 * 1000;
    this.excerptChars = options.excerptChars ?? 500;
    this.maxPassageChars = options.maxPassageChars ?? 1200;
  }

  async retrieve(term: string, sourceKeys?: string[]): Promise<LiveRetrievalResult> {
    const keys = this.liveSourceKeys.filter(
      (key) => this.fetchers.has(key) && (!sourceKeys || sourceKeys.includes(key))
    );
    const outcomes = await Promise.all(keys.map((key) => this.fromSource(key, term)));
    return {
      passages: outcomes.flatMap((outcome) => outcome.passages),
      sources: outcomes.map((outcome) => outcome.status)
    };
  }

  // The term is hashed so search text never appears in Redis keys or in the
  // cache's key logs.
  static cacheKey(sourceKey: string, term: string): string {
    const normalized = term.trim().toLowerCase().split(/\s/).filter((w) => w.length > 0).join(' ');
    return `mlse:live:${sourceKey}:${createHash('sha256').update(normalized).digest('hex')}`;
  }

  private async fromSource(
    sourceKey: string,
    term: string
  ): Promise<{ passages: CitablePassageDto[]; status: LiveSourceResultDto }> {
    const key = LiveRetrievalService.cacheKey(sourceKey, term);

    try {
      if (this.cache && (await this.cache.peekRecord(key))) {
        const documents = (await this.cache.readRecord<FetchedDocumentDto[]>(key)).value;
        return {
          passages: this.toPassages(documents),
          status: { sourceKey, status: 'cached', documents: documents.length }
        };
      }
    } catch {
      // a cache problem must never stop a live query
    }

    try {
      const documents = await this.withTimeout(
        this.fetchers.get(sourceKey)!.fetchDocuments({ term, limit: this.documentsPerSource })
      );
      if (this.cache) {
        await this.cache
          .putRecord({ key, value: documents, ttlMilliseconds: this.cacheTtlMs })
          .catch(() => undefined);
      }
      return {
        passages: this.toPassages(documents),
        status: { sourceKey, status: 'ok', documents: documents.length }
      };
    } catch (error) {
      return {
        passages: [],
        status: {
          sourceKey,
          status: error instanceof TimeoutError ? 'timeout' : 'error',
          documents: 0,
          error: error instanceof TimeoutError ? undefined : (error as Error).message
        }
      };
    }
  }

  private toPassages(documents: FetchedDocumentDto[]): CitablePassageDto[] {
    return documents
      .filter((doc) => !doc.retracted)
      .flatMap((doc) => {
        const licenseScope = licenseScopeFor(doc.license);
        const sections = applyLicense(doc.sections, licenseScope, this.excerptChars);
        return chunkSections(sections, { maxChars: this.maxPassageChars }).map((passage) => ({
          passageId: `live:${doc.sourceKey}:${doc.externalId}:${passage.ordinal}`,
          origin: 'live' as const,
          sourceKey: doc.sourceKey,
          externalId: doc.externalId,
          title: doc.title,
          url: doc.url,
          publishedAt: doc.publishedAt,
          isCaseReport: doc.isCaseReport ?? false,
          licenseScope,
          sectionPath: passage.sectionPath,
          text: passage.text
        }));
      });
  }

  private withTimeout<T>(promise: Promise<T>): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new TimeoutError()), this.timeoutMs);
    });
    return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
  }
}
