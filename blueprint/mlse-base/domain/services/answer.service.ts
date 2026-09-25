import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import {
  ANSWER_INSTRUCTIONS,
  classifyQuery,
  DOSAGE_NO_CONTEXT_MESSAGE,
  DRAFT_ANSWER_NOTICE,
  EMERGENCY_MESSAGE,
  INSUFFICIENT_EVIDENCE_MARKER,
  LlmProvider,
  PATIENT_SPECIFIC_MESSAGE,
  PRESCRIPTION_MESSAGE,
  QUOTED_NOTICE,
  queryTerms,
  sectionPrompt,
  selfCheckPrompt,
  SOURCE_NOT_FOUND_MESSAGE,
  verifyDraft
} from '@forklaunch/implementation-mlse-base/services';
import {
  AnswerKind,
  AnswerRequestDto,
  AnswerResponseDto,
  AnswerSectionDto,
  AnswerStreamEventDto,
  CitablePassageDto,
  QueryClassificationDto
} from '@forklaunch/interfaces-mlse/types';
import { EntityManager } from '@mikro-orm/core';
import { countMetric } from '../metrics';
import { tenantEm } from '../tenantEm';
import { SearchHistory } from '../../persistence/entities/searchHistory.entity';
import { AnswerCitation } from '../../persistence/entities/answerCitation.entity';
import {
  GeneratedAnswer,
  RemovedSentenceRecord
} from '../../persistence/entities/generatedAnswer.entity';
import { SearchService } from './search.service';
import { TopicService } from './topic.service';

export type AnswerServiceOptions = {
  // sections drafted at the same time
  concurrency?: number;
  // passages given to the AI for a free-text query
  passagesPerAnswer?: number;
  // passed to verifyDraft
  minSupport?: number;
};

type SectionItem = { key: string; label: string; number?: number };

type SectionResult = {
  section: AnswerSectionDto;
  cited: CitablePassageDto[];
  removed: RemovedSentenceRecord[];
  models: string[];
};

// Words that describe the patient rather than the drug; dropped before the
// label lookup so "my patient weighs 80 kg, propofol" looks up propofol.
const PATIENT_WORDS = new Set(
  queryTerms(
    'patient patients weigh weighs weight kg kilo kilos lb lbs pound year old month week give should much many start adult child man woman him her he she'
  )
);

const LABEL_SOURCES = ['openfda', 'dailymed'];
const LABEL_PASSAGES = 2;

/**
 * Answers a doctor's query.
 *
 * 1. The query is classified by fixed rules before anything else. Emergency,
 *    prescription and patient-specific queries never reach the AI: they get
 *    fixed messages, and dosing questions get the label's dosing section
 *    quoted as written.
 * 2. Literature questions are answered from retrieved passages only: the
 *    search results for a free-text query, or each question's and phase's
 *    evidence for a topic page.
 * 3. Every drafted sentence is verified (citations exist and support it,
 *    numbers match the source). Failed sentences are removed; if any were,
 *    the section is drafted once more with the failures listed (self-check)
 *    and the better verified draft is kept.
 * 4. The answer, its citations and every removed sentence are recorded for
 *    audit.
 */
export class AnswerService {
  private readonly concurrency: number;
  private readonly passagesPerAnswer: number;
  private readonly minSupport: number | undefined;

  constructor(
    private readonly em: EntityManager,
    private readonly searchService: SearchService,
    private readonly topicService: TopicService,
    private readonly llmProvider: LlmProvider,
    private readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    options: AnswerServiceOptions = {}
  ) {
    this.concurrency = Math.max(1, options.concurrency ?? 4);
    this.passagesPerAnswer = options.passagesPerAnswer ?? 8;
    this.minSupport = options.minSupport;
  }

  async answer(request: AnswerRequestDto): Promise<AnswerResponseDto> {
    for await (const event of this.stream(request)) {
      if (event.type === 'done') {
        return event.answer;
      }
    }
    throw new Error('Answer stream ended without a result');
  }

  async *stream(request: AnswerRequestDto): AsyncGenerator<AnswerStreamEventDto> {
    const started = Date.now();
    const query = request.query.trim();
    const classification = classifyQuery(query);

    const fixed = await this.fixedResponse(classification, request);
    if (fixed) {
      yield { type: 'start', queryClass: classification.queryClass, kind: fixed.kind, message: fixed.message };
      for (const section of fixed.sections) {
        yield { type: 'section', section };
      }
      const answer = await this.record({
        query,
        classification,
        kind: fixed.kind,
        message: fixed.message,
        notice: QUOTED_NOTICE,
        results: fixed.sections.map((section) => ({ section, cited: fixed.cited, removed: [], models: [] })),
        started,
        usedAi: false,
        request
      });
      yield { type: 'done', answer };
      return;
    }

    yield { type: 'start', queryClass: classification.queryClass, kind: 'answer' };

    const { items, evidenceFor, topic, notice } = await this.sectionsToAnswer(request, query);
    const pending = startLimited(
      items.map((item) => () => this.draftSection(item, evidenceFor(item), { query, topic })),
      this.concurrency
    );
    const results: SectionResult[] = [];
    for (const next of pending) {
      const result = await next;
      results.push(result);
      yield { type: 'section', section: result.section };
    }

    const answer = await this.record({
      query,
      classification,
      kind: 'answer',
      notice: notice ? `${DRAFT_ANSWER_NOTICE} ${notice}` : DRAFT_ANSWER_NOTICE,
      results,
      started,
      usedAi: true,
      request,
      ...(request.topicSlug ? { topicSlug: request.topicSlug } : {})
    });
    yield { type: 'done', answer };
  }

  // Responses that are not written by the AI, or undefined for a literature
  // question.
  private async fixedResponse(
    classification: QueryClassificationDto,
    request: AnswerRequestDto
  ): Promise<{ kind: AnswerKind; message: string; sections: AnswerSectionDto[]; cited: CitablePassageDto[] } | undefined> {
    switch (classification.queryClass) {
      case 'emergency_pattern':
        return { kind: 'emergency', message: EMERGENCY_MESSAGE, sections: [], cited: [] };
      case 'prescription_request':
        return { kind: 'boundary', message: PRESCRIPTION_MESSAGE, sections: [], cited: [] };
      case 'patient_specific_treatment': {
        const label = await this.labelDosing(classification, request);
        return { kind: 'boundary', message: PATIENT_SPECIFIC_MESSAGE, ...label };
      }
      case 'exact_dosage_no_context': {
        const label = await this.labelDosing(classification, request);
        return { kind: 'label_range', message: DOSAGE_NO_CONTEXT_MESSAGE, ...label };
      }
      default:
        break;
    }
    if (classification.sourceReferences.length > 0 && !(await this.holdsAnySource(classification.sourceReferences))) {
      return { kind: 'source_not_found', message: SOURCE_NOT_FOUND_MESSAGE, sections: [], cited: [] };
    }
    return undefined;
  }

  // The label's dosing section for the drug named in the query, quoted as
  // written. No AI and no arithmetic.
  private async labelDosing(
    classification: QueryClassificationDto,
    request: AnswerRequestDto
  ): Promise<{ sections: AnswerSectionDto[]; cited: CitablePassageDto[] }> {
    const drugTerms = classification.subjectTerms.filter((term) => !PATIENT_WORDS.has(term));
    const section: AnswerSectionDto = {
      key: 'label_dosing',
      label: 'Label dosing section (quoted)',
      status: 'insufficient_evidence',
      sentences: [],
      removed: 0
    };
    if (drugTerms.length === 0) {
      return { sections: [section], cited: [] };
    }
    const { results } = await this.searchService.search({
      query: `${drugTerms.join(' ')} dosage and administration`,
      sourceKeys: LABEL_SOURCES,
      limit: 10,
      live: request.live ?? true,
      ...(request.organizationId ? { organizationId: request.organizationId } : {})
    });
    const cited = results
      .filter((r) => r.licenseScope !== 'metadata_only' && /dosage/i.test(r.sectionPath))
      .filter((r) => {
        const words = new Set(queryTerms(`${r.title} ${r.text}`));
        return drugTerms.some((term) => words.has(term));
      })
      .slice(0, LABEL_PASSAGES);
    if (cited.length === 0) {
      return { sections: [section], cited: [] };
    }
    return {
      sections: [
        {
          ...section,
          status: 'answered',
          sentences: cited.map((p) => ({ text: p.text, citations: [p.passageId], quoted: true }))
        }
      ],
      cited
    };
  }

  private async holdsAnySource(references: string[]): Promise<boolean> {
    const externalIds = references.map((ref) => (ref.startsWith('PMID:') ? ref.slice(5) : ref));
    const dois = references.filter((ref) => ref.startsWith('10.'));
    const rows: { found: number }[] = await this.em.getConnection().execute(
      `select count(*)::int as found from document
        where status = 'current'
          and (upper(external_id) in (${externalIds.map(() => 'upper(?)').join(', ')})
               ${dois.map(() => 'or lower(url) like ?').join(' ')})`,
      [...externalIds, ...dois.map((doi) => `%${doi.toLowerCase().replace(/[%_\\]/g, '\\$&')}%`)]
    );
    return (rows[0]?.found ?? 0) > 0;
  }

  private async sectionsToAnswer(request: AnswerRequestDto, query: string) {
    if (request.topicSlug) {
      const page = await this.topicService.getPage(request.topicSlug);
      const items = [...page.questions, ...page.phases];
      const byKey = new Map(items.map((item) => [item.key, item.evidence]));
      return {
        items: items.map((item) => ({
          key: item.key,
          label: item.label,
          ...('number' in item && item.number !== undefined ? { number: item.number } : {})
        })),
        evidenceFor: (item: SectionItem): CitablePassageDto[] =>
          (byKey.get(item.key) ?? []).map((e) => ({ ...e, origin: 'corpus' as const, licenseScope: e.licenseScope as CitablePassageDto['licenseScope'] })),
        topic: page.title,
        notice: page.notice
      };
    }

    const { results } = await this.searchService.search({
      query,
      limit: this.passagesPerAnswer,
      live: request.live ?? true,
      ...(request.organizationId ? { organizationId: request.organizationId } : {})
    });
    const usable = results.filter((r) => r.licenseScope !== 'metadata_only' && r.text.trim().length > 0);
    return {
      items: [{ key: 'answer', label: query }] as SectionItem[],
      evidenceFor: (): CitablePassageDto[] => usable,
      topic: undefined,
      notice: undefined
    };
  }

  private async draftSection(
    item: SectionItem,
    passages: CitablePassageDto[],
    context: { query: string; topic: string | undefined }
  ): Promise<SectionResult> {
    const base: AnswerSectionDto = {
      key: item.key,
      label: item.label,
      ...(item.number !== undefined ? { number: item.number } : {}),
      status: 'insufficient_evidence',
      sentences: [],
      removed: 0
    };
    if (passages.length === 0) {
      return { section: base, cited: [], removed: [], models: [] };
    }

    // short ids keep the model from mistyping long passage ids
    const byShortId = new Map(passages.map((p, i) => [`P${i + 1}`, p]));
    const texts = new Map([...byShortId].map(([id, p]) => [id, p.text]));
    const evidence = [...byShortId].map(([id, p]) => ({ id, text: p.text, label: passageLabel(p) }));
    const prompt = sectionPrompt({
      question: item.label,
      ...(context.topic ? { topic: context.topic } : {}),
      query: context.query
    });
    const verify = (text: string) => {
      const lines = text.split('\n').filter((line) => line.trim() !== INSUFFICIENT_EVIDENCE_MARKER);
      return verifyDraft(lines.join('\n'), texts, this.minSupport !== undefined ? { minSupport: this.minSupport } : {});
    };

    const models: string[] = [];
    const removed: RemovedSentenceRecord[] = [];
    try {
      const first = await this.llmProvider.generate({ instructions: ANSWER_INSTRUCTIONS, prompt, evidence });
      models.push(first.model);
      let best = verify(first.text);
      removed.push(...best.removed.map((r) => ({ sectionKey: item.key, text: r.text, reason: r.reason })));

      if (best.removed.length > 0) {
        const second = await this.llmProvider.generate({
          instructions: ANSWER_INSTRUCTIONS,
          prompt: selfCheckPrompt(prompt, best.removed),
          evidence
        });
        models.push(second.model);
        const retried = verify(second.text);
        removed.push(
          ...retried.removed.map((r) => ({ sectionKey: item.key, text: r.text, reason: `self-check: ${r.reason}` }))
        );
        if (retried.kept.length > best.kept.length || (retried.kept.length === best.kept.length && retried.removed.length < best.removed.length)) {
          best = retried;
        }
      }

      const cited = new Map<string, CitablePassageDto>();
      const sentences = best.kept.map((sentence) => ({
        text: sentence.text,
        citations: sentence.citations.map((id) => {
          const passage = byShortId.get(id) as CitablePassageDto;
          cited.set(passage.passageId, passage);
          return passage.passageId;
        })
      }));
      return {
        section: {
          ...base,
          status: sentences.length > 0 ? 'answered' : 'insufficient_evidence',
          sentences,
          removed: best.removed.length
        },
        cited: [...cited.values()],
        removed,
        models
      };
    } catch (error) {
      this.openTelemetryCollector.error('Answer section drafting failed', {
        section: item.key,
        error: error instanceof Error ? error.message : String(error)
      });
      return { section: { ...base, status: 'generation_failed' }, cited: [], removed, models };
    }
  }

  private async record(input: {
    query: string;
    classification: QueryClassificationDto;
    kind: AnswerKind;
    message?: string;
    notice: string;
    results: SectionResult[];
    started: number;
    usedAi: boolean;
    request: AnswerRequestDto;
    topicSlug?: string;
  }): Promise<AnswerResponseDto> {
    const sections = input.results.map((r) => r.section);
    const sources = new Map<string, CitablePassageDto>();
    for (const result of input.results) {
      for (const passage of result.cited) {
        sources.set(passage.passageId, passage);
      }
    }
    const removed = input.results.flatMap((r) => r.removed);
    const models = [...new Set(input.results.flatMap((r) => r.models))];
    const kept = sections.reduce((sum, s) => sum + s.sentences.length, 0);
    const storesQuery =
      input.classification.queryClass === 'literature_lookup' ||
      input.classification.queryClass === 'exact_dosage_no_context';

    const answer = this.em.create(GeneratedAnswer, {
      query: storesQuery ? input.query : null,
      queryClass: input.classification.queryClass,
      classificationReason: input.classification.reason,
      kind: input.kind,
      topicSlug: input.topicSlug ?? null,
      provider: input.usedAi ? this.llmProvider.describe().provider : null,
      model: models.length > 0 ? models.join(', ') : null,
      sections,
      sentencesKept: kept,
      sentencesRemoved: removed.length,
      removedSentences: removed,
      durationMs: Date.now() - input.started
    });
    for (const passage of sources.values()) {
      this.em.create(AnswerCitation, {
        answer,
        passageId: passage.passageId,
        origin: passage.origin,
        sourceKey: passage.sourceKey,
        externalId: passage.externalId,
        title: passage.title,
        url: passage.url,
        sectionPath: passage.sectionPath,
        licenseScope: passage.licenseScope
      });
    }
    await this.em.flush();

    const { organizationId, userId } = input.request;
    if (organizationId && userId) {
      const em = tenantEm(this.em, organizationId);
      em.create(SearchHistory, {
        organizationId,
        userId,
        query: storesQuery ? input.query : null,
        queryClass: input.classification.queryClass,
        channel: input.request.channel ?? 'answer',
        answerId: answer.id
      });
      await em.flush();
    }

    countMetric(this.openTelemetryCollector, 'mlse_answers_total', 1, {
      kind: input.kind,
      query_class: input.classification.queryClass
    });
    if (removed.length > 0) {
      countMetric(this.openTelemetryCollector, 'mlse_sentences_removed_total', removed.length, {
        query_class: input.classification.queryClass
      });
    }

    // the query itself is not logged
    this.openTelemetryCollector.info('Answer produced', {
      answerId: answer.id,
      queryClass: input.classification.queryClass,
      kind: input.kind,
      sections: sections.length,
      sentencesKept: kept,
      sentencesRemoved: removed.length,
      durationMs: Date.now() - input.started
    });

    return {
      answerId: answer.id,
      query: input.query,
      queryClass: input.classification.queryClass,
      kind: input.kind,
      ...(input.message ? { message: input.message } : {}),
      notice: input.notice,
      sections,
      sources: [...sources.values()],
      ...(models.length > 0 ? { model: models.join(', ') } : {})
    };
  }
}

function passageLabel(passage: CitablePassageDto): string {
  const kind = passage.isCaseReport ? 'case report' : passage.sourceKey === 'openfda' || passage.sourceKey === 'dailymed' ? 'drug label' : 'article';
  return `${passage.sourceKey} ${kind}: ${passage.title} - ${passage.sectionPath}`;
}

// Starts at most `limit` tasks at once; the promises come back in task order
// so sections stream in the order of the page.
function startLimited<T>(tasks: (() => Promise<T>)[], limit: number): Promise<T>[] {
  let active = 0;
  const waiting: (() => void)[] = [];
  return tasks.map(
    (task) =>
      new Promise<T>((resolve, reject) => {
        const run = () => {
          active++;
          task()
            .then(resolve, reject)
            .finally(() => {
              active--;
              waiting.shift()?.();
            });
        };
        if (active < limit) run();
        else waiting.push(run);
      })
  );
}
