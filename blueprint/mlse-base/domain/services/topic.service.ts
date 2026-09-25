import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import {
  caseRelevance,
  diagnosisGroup,
  extractCaseStudyFields,
  extractQuantities,
  frameworkItems,
  QUESTION_FRAMEWORKS,
  queryTerms,
  selectEvidence
} from '@forklaunch/implementation-mlse-base/services';
import { SearchResultDto } from '@forklaunch/interfaces-mlse/types';
import { EntityManager } from '@mikro-orm/core';
import { CaseStudy } from '../../persistence/entities/caseStudy.entity';
import { Document } from '../../persistence/entities/document.entity';
import { DocumentChunk } from '../../persistence/entities/documentChunk.entity';
import { MedicalConcept } from '../../persistence/entities/medicalConcept.entity';
import { QuantitativeFact } from '../../persistence/entities/quantitativeFact.entity';
import { Topic } from '../../persistence/entities/topic.entity';
import { TopicEvidence } from '../../persistence/entities/topicEvidence.entity';
import {
  LICENSED_SOURCE_ACCESS_SQL,
  OPEN_FLAG_EXCLUSION_SQL,
  SearchService
} from './search.service';

export class TopicNotFoundError extends Error {
  constructor(readonly slug: string) {
    super(`Topic '${slug}' not found`);
    this.name = 'TopicNotFoundError';
  }
}

export type AssemblyResult = {
  slug: string;
  items: number;
  withEvidence: number;
  insufficientEvidence: string[];
  facts: number;
  caseStudies: number;
  casesExcluded: number;
};

const EVIDENCE_PER_ITEM = 3;
const SEARCH_DEPTH = 15;

/**
 * Builds and reads topic pages.
 *
 * Assembly maps every question and phase of the topic's framework to stored
 * passages that address it. A passage counts only if it mentions one of the
 * item's hint words and its document concerns the topic; an item with no
 * such passage is recorded as insufficient evidence rather than filled in.
 * Numbers in the evidence of quantitative items are extracted for review,
 * and related case reports are attached, grouped by diagnosis.
 *
 * Assembly draws on the stored corpus only, so every citation is a durable
 * passage id. Live results reach the corpus through ingestion.
 */
export class TopicService {
  constructor(
    private readonly em: EntityManager,
    private readonly searchService: SearchService,
    private readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async listTopics() {
    const topics = await this.em.find(Topic, {}, { orderBy: { title: 'asc' } });
    return topics.map((topic) => ({
      slug: topic.slug,
      title: topic.title,
      topicType: topic.topicType,
      status: topic.status,
      ...(topic.assembledAt ? { assembledAt: new Date(topic.assembledAt).toISOString() } : {})
    }));
  }

  async assemble(slug: string): Promise<AssemblyResult> {
    const topic = await this.em.findOne(Topic, { slug });
    if (!topic) {
      throw new TopicNotFoundError(slug);
    }
    const framework = QUESTION_FRAMEWORKS[topic.frameworkKey];
    if (!framework) {
      throw new Error(`Topic '${slug}' uses unknown framework '${topic.frameworkKey}'`);
    }

    const evidence: { itemKey: string; itemKind: string; result: SearchResultDto; rank: number }[] = [];
    const insufficientEvidence: string[] = [];
    for (const item of frameworkItems(framework)) {
      const { results } = await this.searchService.search({
        query: `${topic.title} ${item.searchHints.join(' ')}`,
        limit: SEARCH_DEPTH,
        live: false
      });
      const kept = selectEvidence(
        results
          .filter((r) => r.origin === 'corpus')
          .map((r) => ({ ...r, documentKey: `${r.sourceKey}:${r.externalId}` })),
        { hints: item.searchHints, topicTerms: topic.searchTerms, limit: EVIDENCE_PER_ITEM }
      );
      if (kept.length === 0) {
        insufficientEvidence.push(item.key);
      }
      kept.forEach((result, rank) => evidence.push({ itemKey: item.key, itemKind: item.kind, result, rank }));
    }

    // hint words per quantitative item: a number is kept only if its own
    // sentence mentions one ("median blood loss was 20 mL" for blood, not
    // "60% of patients were female")
    const quantitativeHints = new Map(
      frameworkItems(framework)
        .filter((item) => item.quantitative)
        .map((item) => [item.key, new Set(item.searchHints.flatMap((hint) => queryTerms(hint)))])
    );

    const cases = await this.findCaseStudies(topic);

    await this.em.transactional(async (em) => {
      await em.nativeDelete(TopicEvidence, { topic: topic.id });
      await em.nativeDelete(QuantitativeFact, { topic: topic.id });
      await em.nativeDelete(CaseStudy, { topic: topic.id });

      for (const { itemKey, itemKind, result, rank } of evidence) {
        const chunk = em.getReference(DocumentChunk, result.passageId);
        const chunkRow = await em.findOneOrFail(DocumentChunk, { id: result.passageId }, { populate: ['document'] });
        em.create(TopicEvidence, {
          topic: em.getReference(Topic, topic.id),
          itemKey,
          itemKind,
          chunk,
          document: chunkRow.document,
          rank,
          score: result.score
        });
        const hints = quantitativeHints.get(itemKey);
        if (hints) {
          const seen = new Set<string>();
          for (const quantity of extractQuantities(result.text)) {
            if (seen.has(quantity.raw)) continue;
            if (!queryTerms(quantity.sentence).some((word) => hints.has(word))) continue;
            seen.add(quantity.raw);
            em.create(QuantitativeFact, {
              topic: em.getReference(Topic, topic.id),
              itemKey,
              chunk,
              document: chunkRow.document,
              raw: quantity.raw,
              low: quantity.low,
              high: quantity.high,
              unit: quantity.unit,
              statistic: quantity.statistic ?? null,
              sentence: quantity.sentence,
              population: null,
              technique: null,
              reviewStatus: 'unreviewed'
            });
          }
        }
      }

      for (const found of cases.relevant) {
        em.create(CaseStudy, {
          topic: em.getReference(Topic, topic.id),
          document: em.getReference(Document, found.documentId),
          diagnosis: found.diagnosis,
          relevanceReason: found.reason,
          presentation: found.fields.presentation ?? null,
          diagnosisText: found.fields.diagnosis ?? null,
          management: found.fields.management ?? null,
          outcome: found.fields.outcome ?? null
        });
      }

      const managed = await em.findOneOrFail(Topic, { id: topic.id });
      managed.assembledAt = new Date();
    });

    const facts = await this.em.count(QuantitativeFact, { topic: topic.id });
    const result: AssemblyResult = {
      slug,
      items: frameworkItems(framework).length,
      withEvidence: frameworkItems(framework).length - insufficientEvidence.length,
      insufficientEvidence,
      facts,
      caseStudies: cases.relevant.length,
      casesExcluded: cases.excluded
    };
    this.openTelemetryCollector.info('Topic assembled', result);
    return result;
  }

  // Current case reports that mention the topic, checked for real relevance
  // and grouped by the other MeSH descriptors they carry.
  private async findCaseStudies(topic: Topic) {
    const candidates: { id: string; title: string; mesh_descriptor_uis: string[] }[] = await this.em.getConnection().execute(
      `select distinct d.id, d.title, d.mesh_descriptor_uis
         from document d
         left join document_chunk c on c.document_id = d.id
        where d.status = 'current' and d.is_case_report
          -- topic pages are shared by every organization, so licensed
          -- content and flagged documents never become case studies
          and ${LICENSED_SOURCE_ACCESS_SQL} and ${OPEN_FLAG_EXCLUSION_SQL}
          and (? = any(d.mesh_descriptor_uis)
               or c.search_vector @@ (${topic.searchTerms.map(() => `plainto_tsquery('english', ?)`).join(' || ')})
               or to_tsvector('english', d.title) @@ (${topic.searchTerms.map(() => `plainto_tsquery('english', ?)`).join(' || ')}))`,
      ['', topic.meshDescriptorUi ?? '', ...topic.searchTerms, ...topic.searchTerms]
    );

    const relevant: {
      documentId: string;
      diagnosis: string;
      reason: 'mesh' | 'terms';
      fields: ReturnType<typeof extractCaseStudyFields>;
    }[] = [];
    let excluded = 0;

    for (const candidate of candidates) {
      const chunks = await this.em.find(
        DocumentChunk,
        { document: candidate.id },
        { orderBy: { ordinal: 'asc' } }
      );
      const text = chunks.map((c) => c.text).join(' ');
      const relevance = caseRelevance(
        {
          meshDescriptorUi: topic.meshDescriptorUi,
          searchTerms: topic.searchTerms
        },
        { meshDescriptorUis: candidate.mesh_descriptor_uis ?? [], title: candidate.title, text }
      );
      if (!relevance.relevant || relevance.reason === 'none') {
        excluded++;
        continue;
      }

      const otherDescriptors = (candidate.mesh_descriptor_uis ?? []).filter(
        (ui: string) => ui !== topic.meshDescriptorUi
      );
      const concepts = otherDescriptors.length
        ? await this.em.find(MedicalConcept, { descriptorUi: { $in: otherDescriptors } })
        : [];
      const fields = extractCaseStudyFields(
        chunks.map((c) => ({ path: c.sectionPath, text: c.text }))
      );
      if (!fields.presentation && chunks.length > 0) {
        // an unlabelled abstract still gives a doctor the case in brief
        fields.presentation = chunks[0].text;
      }

      relevant.push({
        documentId: candidate.id,
        diagnosis: diagnosisGroup(
          concepts.map((c) => ({ preferredTerm: c.preferredTerm, treeNumbers: c.treeNumbers }))
        ),
        reason: relevance.reason,
        fields
      });
    }
    return { relevant, excluded };
  }

  async getPage(slug: string, phaseNumber?: number) {
    const topic = await this.em.findOne(Topic, { slug });
    if (!topic) {
      throw new TopicNotFoundError(slug);
    }
    const framework = QUESTION_FRAMEWORKS[topic.frameworkKey];
    if (!framework) {
      throw new Error(`Topic '${slug}' uses unknown framework '${topic.frameworkKey}'`);
    }

    type EvidenceRow = {
        item_key: string;
        rank: number;
        score: number;
        chunk_id: string;
        section_path: string;
        text: string;
        source_key: string;
        external_id: string;
        title: string;
        url: string;
        published_at: string | null;
        is_case_report: boolean;
        license_scope: string;
      };
    const evidenceRows: EvidenceRow[] = await this.em.getConnection().execute(
      `select e.item_key, e.rank, e.score, c.id as chunk_id, c.section_path, c.text,
              d.source_key, d.external_id, d.title, d.url, d.published_at, d.is_case_report, d.license_scope
         from topic_evidence e
         join document_chunk c on c.id = e.chunk_id
         join document d on d.id = e.document_id
        where e.topic_id = ? and d.status = 'current'
          and not exists (select 1 from content_flag f where f.document_id = d.id and f.status = 'open')
        order by e.item_key, e.rank`,
      [topic.id]
    );
    // documents with an open reviewer flag stay hidden in every part of the
    // page: evidence (filtered in the query above), numbers and case studies
    const flaggedRows: { document_id: string }[] = await this.em
      .getConnection()
      .execute(`select distinct document_id from content_flag where status = 'open'`);
    const flagged = new Set(flaggedRows.map((row) => row.document_id));
    const facts = await this.em.find(
      QuantitativeFact,
      { topic: topic.id, document: { status: 'current' } },
      { orderBy: { itemKey: 'asc', createdAt: 'asc' } }
    );
    const caseStudies = await this.em.find(
      CaseStudy,
      { topic: topic.id, document: { status: 'current' } },
      { populate: ['document'], orderBy: { diagnosis: 'asc' } }
    );

    const itemView = (item: { key: string; label: string; number?: number }) => {
      const evidence = evidenceRows
        .filter((row) => row.item_key === item.key)
        .map((row) => ({
          passageId: row.chunk_id,
          sourceKey: row.source_key,
          externalId: row.external_id,
          title: row.title,
          url: row.url,
          ...(row.published_at ? { publishedAt: row.published_at } : {}),
          isCaseReport: row.is_case_report,
          licenseScope: row.license_scope,
          sectionPath: row.section_path,
          text: row.text
        }));
      return {
        key: item.key,
        label: item.label,
        ...(item.number !== undefined ? { number: item.number } : {}),
        status: evidence.length > 0 ? 'evidence_found' : 'insufficient_evidence',
        evidence,
        facts: facts
          .filter((fact) => fact.itemKey === item.key)
          .filter((fact) => !flagged.has((fact.document as unknown as { id: string }).id))
          .map((fact) => ({
            raw: fact.raw,
            // double precision columns are typed number | string by the ORM
            low: Number(fact.low),
            high: Number(fact.high),
            unit: fact.unit,
            ...(fact.statistic ? { statistic: fact.statistic } : {}),
            sentence: fact.sentence,
            reviewStatus: fact.reviewStatus,
            passageId: (fact.chunk as unknown as { id: string }).id
          }))
      };
    };

    const phases = (framework.phases ?? []).filter(
      (phase) => phaseNumber === undefined || phase.number === phaseNumber
    );

    type CaseView = {
      sourceKey: string;
      externalId: string;
      title: string;
      url: string;
      publishedAt?: string;
      licenseScope: string;
      relevanceReason: string;
      presentation?: string;
      diagnosis?: string;
      management?: string;
      outcome?: string;
    };
    const groups = new Map<string, CaseView[]>();
    for (const cs of caseStudies.filter((c) => !flagged.has((c.document as unknown as Document).id))) {
      const doc = cs.document as unknown as Document;
      const list = groups.get(cs.diagnosis) ?? [];
      list.push({
        sourceKey: doc.sourceKey,
        externalId: doc.externalId,
        title: doc.title,
        url: doc.url,
        ...(doc.publishedAt ? { publishedAt: doc.publishedAt } : {}),
        licenseScope: doc.licenseScope,
        relevanceReason: cs.relevanceReason,
        ...(cs.presentation ? { presentation: cs.presentation } : {}),
        ...(cs.diagnosisText ? { diagnosis: cs.diagnosisText } : {}),
        ...(cs.management ? { management: cs.management } : {}),
        ...(cs.outcome ? { outcome: cs.outcome } : {})
      });
      groups.set(cs.diagnosis, list);
    }

    return {
      slug: topic.slug,
      title: topic.title,
      topicType: topic.topicType,
      status: topic.status,
      framework: { key: framework.key, status: framework.status },
      ...(topic.status === 'approved' && framework.status === 'approved'
        ? {}
        : {
            notice:
              'Draft: not yet approved by a clinician. Evidence links are automatic and numbers are unreviewed; not for clinical use.'
          }),
      ...(topic.assembledAt ? { assembledAt: new Date(topic.assembledAt).toISOString() } : {}),
      questions: phaseNumber === undefined ? framework.questions.map(itemView) : [],
      phases: phases.map(itemView),
      caseStudies:
        phaseNumber === undefined
          ? [...groups.entries()].map(([diagnosis, cases]) => ({
              diagnosis,
              evidenceLevel: 'case report (low)',
              cases
            }))
          : []
    };
  }
}
