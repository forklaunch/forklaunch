import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import {
  applyLicense,
  chunkSections,
  contentHash,
  LicensedContentAdapter,
  licenseScopeFor,
  LlmProvider,
  SourceFetcherRegistry
} from '@forklaunch/implementation-mlse-base/services';
import {
  FetchedDocumentDto,
  MeshConceptDto,
  SourceDescriptorDto
} from '@forklaunch/interfaces-mlse/types';
import { EntityManager } from '@mikro-orm/core';
import { DocumentStatus } from '../enum/documentStatus.enum';
import { Document } from '../../persistence/entities/document.entity';
import { DocumentChunk } from '../../persistence/entities/documentChunk.entity';
import { MedicalConcept } from '../../persistence/entities/medicalConcept.entity';
import { Source } from '../../persistence/entities/source.entity';

export class UnknownSourceError extends Error {
  constructor(readonly sourceKey: string) {
    super(`Unknown or non-fetchable content source '${sourceKey}'`);
    this.name = 'UnknownSourceError';
  }
}

export type IngestionRequest = {
  sourceKey: string;
  term: string;
  limit: number;
};

export type IngestionResult = {
  sourceKey: string;
  fetched: number;
  created: number;
  updated: number;
  unchanged: number;
  retracted: number;
  metadataOnly: number;
  passages: number;
};

export type IngestionOptions = {
  // longest excerpt kept from an excerpt-only document (PubMed abstracts)
  excerptChars?: number;
  maxPassageChars?: number;
  embedBatchSize?: number;
};

/**
 * Turns fetched documents into stored, searchable passages, identically for
 * every source:
 *
 * 1. License gate: metadata-only documents keep no text; excerpt-only
 *    documents keep a short excerpt.
 * 2. Deduplication by source id and content hash: unchanged documents are
 *    skipped, changed ones get a new version and the old one is superseded.
 * 3. Retractions: a retracted document keeps its record but loses its
 *    passages, so it leaves search at once.
 * 4. Section-aware chunking and embedding.
 */
export class LicenseRequiredError extends Error {
  constructor(readonly sourceKey: string) {
    super(`'${sourceKey}' is a licensed source and no organization holds an active license for it`);
    this.name = 'LicenseRequiredError';
  }
}

export class IngestionService {
  private readonly excerptChars: number;
  private readonly maxPassageChars: number;
  private readonly embedBatchSize: number;

  constructor(
    private readonly em: EntityManager,
    private readonly fetchers: SourceFetcherRegistry,
    private readonly llmProvider: LlmProvider,
    private readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    options: IngestionOptions = {}
  ) {
    this.excerptChars = options.excerptChars ?? 500;
    this.maxPassageChars = options.maxPassageChars ?? 1200;
    this.embedBatchSize = options.embedBatchSize ?? 32;
  }

  // Keeps the source registry in step with the provider's list.
  async syncSources(descriptors: SourceDescriptorDto[]): Promise<void> {
    for (const descriptor of descriptors) {
      const existing = await this.em.findOne(Source, { sourceKey: descriptor.id });
      const values = {
        name: descriptor.name,
        tier: descriptor.tier,
        licenseTerms: descriptor.licenseTerms,
        commercialUse: descriptor.commercialUse,
        liveQuery: descriptor.liveQuery,
        requiresLicense: descriptor.requiresLicense ?? false
      };
      if (existing) {
        this.em.assign(existing, values);
      } else {
        this.em.create(Source, { sourceKey: descriptor.id, lastRefreshedAt: null, ...values });
      }
    }
    await this.em.flush();
  }

  // A source is licensed if its registry row says so or its fetcher is a
  // LicensedContentAdapter, so forgetting to register a licensed source
  // cannot make it public.
  private async mayIngest(sourceKey: string, licensedFetcher: boolean): Promise<boolean> {
    const rows: { requires_license: boolean | null; active: number }[] = await this.em.getConnection().execute(
      `select s.requires_license,
              (select count(*)::int from content_license l
                where l.source_key = s.source_key and l.status = 'active' and l.valid_from <= now()
                  and (l.valid_until is null or l.valid_until > now())) as active
         from source s where s.source_key = ?`,
      [sourceKey]
    );
    const row = rows[0];
    if (licensedFetcher) {
      // must be registered as licensed and licensed to someone
      return row?.requires_license === true && row.active > 0;
    }
    return !row?.requires_license || row.active > 0;
  }

  async ingest({ sourceKey, term, limit }: IngestionRequest): Promise<IngestionResult> {
    const fetcher = this.fetchers.get(sourceKey);
    if (!fetcher) {
      throw new UnknownSourceError(sourceKey);
    }
    // Licensed content is stored only while someone may use it. Checked
    // before fetching, and fails closed when the source row is missing.
    if (!(await this.mayIngest(sourceKey, fetcher instanceof LicensedContentAdapter))) {
      throw new LicenseRequiredError(sourceKey);
    }

    const documents = await fetcher.fetchDocuments({ term, limit });
    const result: IngestionResult = {
      sourceKey,
      fetched: documents.length,
      created: 0,
      updated: 0,
      unchanged: 0,
      retracted: 0,
      metadataOnly: 0,
      passages: 0
    };

    for (const fetched of documents) {
      await this.em.transactional(async (em) => {
        await this.ingestDocument(em, fetched, result);
      });
    }

    const source = await this.em.findOne(Source, { sourceKey });
    if (source) {
      source.lastRefreshedAt = new Date();
      await this.em.flush();
    }

    this.openTelemetryCollector.info('Corpus ingestion finished', result);
    return result;
  }

  private async ingestDocument(
    em: EntityManager,
    fetched: FetchedDocumentDto,
    result: IngestionResult
  ): Promise<void> {
    const licenseScope = licenseScopeFor(fetched.license);
    const sections = applyLicense(fetched.sections, licenseScope, this.excerptChars);
    const hash = contentHash(fetched.title, sections);
    const status = fetched.retracted ? DocumentStatus.RETRACTED : DocumentStatus.CURRENT;

    const live = await em.findOne(Document, {
      sourceKey: fetched.sourceKey,
      externalId: fetched.externalId,
      status: { $ne: DocumentStatus.SUPERSEDED }
    });

    if (live && live.contentHash === hash) {
      if (live.status === status) {
        result.unchanged++;
        return;
      }
      if (status === DocumentStatus.RETRACTED) {
        // same content, now retracted: withdraw it from search
        live.status = DocumentStatus.RETRACTED;
        await em.nativeDelete(DocumentChunk, { document: live.id });
        result.retracted++;
        return;
      }
    }

    if (live) {
      live.status = DocumentStatus.SUPERSEDED;
      live.supersededAt = new Date();
      // the old version leaves the live-version unique index before the new
      // row is written
      await em.flush();
    }

    const document = em.create(Document, {
      sourceKey: fetched.sourceKey,
      externalId: fetched.externalId,
      version: (live?.version ?? 0) + 1,
      title: fetched.title,
      url: fetched.url,
      publishedAt: fetched.publishedAt ?? null,
      license: fetched.license ?? null,
      licenseScope,
      isCaseReport: fetched.isCaseReport ?? false,
      status,
      contentHash: hash,
      meshDescriptorUis: fetched.meshDescriptorUis ?? [],
      supersededAt: null
    });

    if (live) {
      result.updated++;
    } else {
      result.created++;
    }
    if (licenseScope === 'metadata_only') {
      result.metadataOnly++;
    }
    if (status === DocumentStatus.RETRACTED) {
      result.retracted++;
      return;
    }

    const passages = chunkSections(sections, { maxChars: this.maxPassageChars });
    for (let i = 0; i < passages.length; i += this.embedBatchSize) {
      const batch = passages.slice(i, i + this.embedBatchSize);
      const { embeddings, model } = await this.llmProvider.embed({
        texts: batch.map((p) => `${p.sectionPath}: ${p.text}`)
      });
      batch.forEach((passage, j) => {
        em.create(DocumentChunk, {
          document,
          sectionPath: passage.sectionPath,
          ordinal: passage.ordinal,
          text: passage.text,
          embedding: embeddings[j] ?? null,
          embeddingModel: model
        });
      });
    }
    result.passages += passages.length;
  }

  // Inserts or updates MeSH descriptors; returns how many were written.
  async loadMeshConcepts(concepts: MeshConceptDto[]): Promise<number> {
    if (concepts.length === 0) {
      return 0;
    }
    const existing = await this.em.find(MedicalConcept, {
      descriptorUi: { $in: concepts.map((c) => c.descriptorUi) }
    });
    const byUi = new Map(existing.map((concept) => [concept.descriptorUi, concept]));
    for (const concept of concepts) {
      const values = {
        preferredTerm: concept.preferredTerm,
        synonyms: concept.synonyms,
        treeNumbers: concept.treeNumbers
      };
      const current = byUi.get(concept.descriptorUi);
      if (current) {
        this.em.assign(current, values);
      } else {
        this.em.create(MedicalConcept, { descriptorUi: concept.descriptorUi, ...values });
      }
    }
    await this.em.flush();
    return concepts.length;
  }
}
