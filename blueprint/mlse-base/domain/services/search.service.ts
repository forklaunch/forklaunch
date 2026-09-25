import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import {
  LiveRetrievalService,
  LlmProvider,
  reciprocalRankFusion,
  Reranker
} from '@forklaunch/implementation-mlse-base/services';
import {
  CitablePassageDto,
  LicenseScope,
  LiveSourceResultDto,
  SearchRequestDto,
  SearchResponseDto,
  SearchResultDto
} from '@forklaunch/interfaces-mlse/types';
import { EntityManager } from '@mikro-orm/core';

type ChunkRow = {
  id: string;
  section_path: string;
  text: string;
  source_key: string;
  external_id: string;
  title: string;
  url: string;
  published_at: string | null;
  is_case_report: boolean;
  license_scope: LicenseScope;
};

export type SearchOptions = {
  // candidates each retriever contributes before fusion
  candidatesPerRetriever?: number;
  // fused candidates passed to the re-ranker
  rerankDepth?: number;
  maxExpandedTerms?: number;
};

const MAX_LIMIT = 50;

/**
 * Hybrid search over the corpus plus live sources.
 *
 * 1. The query is expanded with the MeSH descriptors it names, so
 *    "celioscopic cholecystectomy" also searches "laparoscopic cholecystectomy".
 * 2. Keyword search (PostgreSQL full text) and vector search (pgvector) each
 *    rank current passages; live retrieval adds passages fetched just now.
 * 3. Reciprocal rank fusion merges the lists; the re-ranker orders the top.
 *
 * Only current documents are searched: superseded versions and retracted
 * documents never appear.
 */
export class SearchService {
  private readonly candidatesPerRetriever: number;
  private readonly rerankDepth: number;
  private readonly maxExpandedTerms: number;

  constructor(
    private readonly em: EntityManager,
    private readonly llmProvider: LlmProvider,
    private readonly reranker: Reranker,
    private readonly liveRetrieval: LiveRetrievalService,
    private readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    options: SearchOptions = {}
  ) {
    this.candidatesPerRetriever = options.candidatesPerRetriever ?? 50;
    this.rerankDepth = options.rerankDepth ?? 40;
    this.maxExpandedTerms = options.maxExpandedTerms ?? 12;
  }

  async search(request: SearchRequestDto): Promise<SearchResponseDto> {
    const query = request.query.trim();
    const limit = Math.min(Math.max(request.limit ?? 10, 1), MAX_LIMIT);
    const expandedTerms = await this.expandQuery(query);

    const [keyword, vector, live] = await Promise.all([
      this.keywordSearch(expandedTerms, request),
      this.vectorSearch(query, request),
      request.live === false
        ? Promise.resolve({ passages: [] as CitablePassageDto[], sources: [] as LiveSourceResultDto[] })
        : this.liveRetrieval.retrieve(query, request.sourceKeys)
    ]);

    // live passages for documents already in the corpus add nothing new
    const livePassages = await this.newLivePassages(
      live.passages.filter((p) => this.passesFilters(p, request))
    );

    const byId = new Map<string, CitablePassageDto>();
    for (const passage of [...keyword, ...vector, ...livePassages]) {
      byId.set(passage.passageId, passage);
    }

    const liveRanking = (
      await this.reranker.rerank(
        expandedTerms.join(' '),
        livePassages.map((p) => ({ id: p.passageId, text: `${p.title} ${p.sectionPath} ${p.text}` }))
      )
    )
      .filter((s) => s.score > 0)
      .sort((a, b) => b.score - a.score)
      .map((s) => s.id);

    const retrievers = ['keyword', 'vector', 'live'] as const;
    const fused = reciprocalRankFusion([
      keyword.map((p) => p.passageId),
      vector.map((p) => p.passageId),
      liveRanking
    ]).slice(0, this.rerankDepth);

    const rerankScores = new Map(
      (
        await this.reranker.rerank(
          query,
          fused.map((f) => {
            const p = byId.get(f.id)!;
            return { id: f.id, text: `${p.title} ${p.sectionPath} ${p.text}` };
          })
        )
      ).map((s) => [s.id, s.score])
    );
    const topFused = fused[0]?.score ?? 1;

    const results: SearchResultDto[] = fused
      .map((f) => ({
        ...byId.get(f.id)!,
        score: 0.6 * (f.score / topFused) + 0.4 * (rerankScores.get(f.id) ?? 0),
        matchedBy: f.lists.map((index) => retrievers[index])
      }))
      .sort((a, b) => b.score - a.score)
      .slice(0, limit);

    this.openTelemetryCollector.debug('Search completed', {
      query,
      expandedTerms: expandedTerms.length,
      keyword: keyword.length,
      vector: vector.length,
      live: livePassages.length,
      returned: results.length
    });

    return { query, expandedTerms, results, liveSources: live.sources };
  }

  // The query plus the preferred terms and synonyms of MeSH descriptors it
  // names, either exactly or as a phrase inside a longer query.
  async expandQuery(query: string): Promise<string[]> {
    const rows = await this.em.getConnection().execute<
      { preferred_term: string; synonyms: string[] }[]
    >(
      `select preferred_term, synonyms from medical_concept
        where lower(preferred_term) = lower(?)
           or exists (select 1 from unnest(synonyms) s where lower(s) = lower(?))
           or (length(preferred_term) >= 4 and position(lower(preferred_term) in lower(?)) > 0)
           or exists (select 1 from unnest(synonyms) s where length(s) >= 4 and position(lower(s) in lower(?)) > 0)
        order by length(preferred_term) desc
        limit 5`,
      [query, query, query, query]
    );

    const terms = new Map<string, string>([[query.toLowerCase(), query]]);
    for (const row of rows) {
      for (const term of [row.preferred_term, ...(row.synonyms ?? [])]) {
        if (!terms.has(term.toLowerCase())) {
          terms.set(term.toLowerCase(), term);
        }
      }
    }
    return [...terms.values()].slice(0, this.maxExpandedTerms);
  }

  private async keywordSearch(
    terms: string[],
    request: SearchRequestDto
  ): Promise<CitablePassageDto[]> {
    // Recall first: a passage matches if it contains any query word or any
    // expanded term's word; ts_rank_cd (cover density) then ranks passages
    // holding more of the words, closer together, above the rest, and the
    // re-ranker checks coverage of the original query. Requiring every word
    // (plain AND) missed relevant passages that phrase things differently.
    const tsquery = terms
      .map(() => `replace(plainto_tsquery('english', ?)::text, ' & ', ' | ')::tsquery`)
      .join(' || ');
    const filter = this.filterSql(request);
    const rows = await this.em.getConnection().execute<ChunkRow[]>(
      `with q as (select (${tsquery}) as query)
       select c.id, c.section_path, c.text, d.source_key, d.external_id, d.title, d.url,
              d.published_at, d.is_case_report, d.license_scope
         from document_chunk c
         join document d on d.id = c.document_id
         cross join q
        where d.status = 'current' and c.search_vector @@ q.query ${filter.sql}
        order by ts_rank_cd(c.search_vector, q.query) desc, c.id
        limit ?`,
      [...terms, ...filter.params, this.candidatesPerRetriever]
    );
    return rows.map(toPassage);
  }

  private async vectorSearch(
    query: string,
    request: SearchRequestDto
  ): Promise<CitablePassageDto[]> {
    let embedding: number[] | undefined;
    let model: string;
    try {
      const response = await this.llmProvider.embed({ texts: [query] });
      embedding = response.embeddings[0];
      model = response.model;
    } catch (error) {
      // keyword and live search still answer if embeddings are unavailable
      this.openTelemetryCollector.error('Query embedding failed', error);
      return [];
    }
    if (!embedding?.length) {
      return [];
    }

    const filter = this.filterSql(request);
    const rows = await this.em.getConnection().execute<ChunkRow[]>(
      `select c.id, c.section_path, c.text, d.source_key, d.external_id, d.title, d.url,
              d.published_at, d.is_case_report, d.license_scope
         from document_chunk c
         join document d on d.id = c.document_id
        where d.status = 'current'
          and c.embedding is not null
          and c.embedding_model = ?
          and vector_dims(c.embedding) = ? ${filter.sql}
        order by c.embedding <-> ?::vector, c.id
        limit ?`,
      [model, embedding.length, ...filter.params, `[${embedding.join(',')}]`, this.candidatesPerRetriever]
    );
    return rows.map(toPassage);
  }

  private filterSql(request: SearchRequestDto): { sql: string; params: unknown[] } {
    const clauses: string[] = [];
    const params: unknown[] = [];
    if (request.sourceKeys?.length) {
      clauses.push(`d.source_key in (${request.sourceKeys.map(() => '?').join(', ')})`);
      params.push(...request.sourceKeys);
    }
    if (request.caseReportsOnly) {
      clauses.push('d.is_case_report');
    }
    if (request.publishedAfter) {
      clauses.push('d.published_at >= ?');
      params.push(request.publishedAfter);
    }
    return { sql: clauses.map((c) => `and ${c}`).join(' '), params };
  }

  private passesFilters(passage: CitablePassageDto, request: SearchRequestDto): boolean {
    if (request.sourceKeys?.length && !request.sourceKeys.includes(passage.sourceKey)) {
      return false;
    }
    if (request.caseReportsOnly && !passage.isCaseReport) {
      return false;
    }
    if (request.publishedAfter && (passage.publishedAt ?? '') < request.publishedAfter) {
      return false;
    }
    return true;
  }

  private async newLivePassages(passages: CitablePassageDto[]): Promise<CitablePassageDto[]> {
    const docs = [...new Map(passages.map((p) => [`${p.sourceKey}:${p.externalId}`, p])).values()];
    if (docs.length === 0) {
      return [];
    }
    const params: string[] = docs.flatMap((p) => [p.sourceKey, p.externalId]);
    const rows: { source_key: string; external_id: string }[] = await this.em
      .getConnection()
      .execute(
        `select source_key, external_id from document
          where status = 'current' and (${docs.map(() => '(source_key = ? and external_id = ?)').join(' or ')})`,
        params
      );
    const stored = new Set(rows.map((r) => `${r.source_key}:${r.external_id}`));
    return passages.filter((p) => !stored.has(`${p.sourceKey}:${p.externalId}`));
  }
}

function toPassage(row: ChunkRow): CitablePassageDto {
  return {
    passageId: row.id,
    origin: 'corpus',
    sourceKey: row.source_key,
    externalId: row.external_id,
    title: row.title,
    url: row.url,
    publishedAt: row.published_at ?? undefined,
    isCaseReport: row.is_case_report,
    licenseScope: row.license_scope,
    sectionPath: row.section_path,
    text: row.text
  };
}
