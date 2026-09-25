import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FakeLlmProvider,
  LexicalReranker,
  LiveRetrievalService,
  SourceFetcher,
  SourceFetcherRegistry
} from '@forklaunch/implementation-mlse-base/services';
import {
  FetchedDocumentDto,
  GenerateRequestDto
} from '@forklaunch/interfaces-mlse/types';
import { MikroORM } from '@mikro-orm/postgresql';
import * as path from 'path';
import { StartedTestContainer } from 'testcontainers';
import { startPgvectorContainer, TEST_DB } from './pgvector-container';

const initOrm = async () => {
  const { default: config } = await import('../mikro-orm.config');
  return MikroORM.init({
    ...config,
    discovery: { ...config.discovery },
    debug: false,
    // no snapshot file: test files run in parallel and would race on it
    migrations: { path: path.join(__dirname, '../migrations'), glob: '!(*.d).{js,ts}', snapshot: false }
  });
};

let container: StartedTestContainer;
let orm: Awaited<ReturnType<typeof initOrm>>;

class StubFetcher implements SourceFetcher {
  documents: FetchedDocumentDto[] = [];
  constructor(readonly sourceKey: string) {}
  async fetchDocuments() {
    return this.documents;
  }
}

// Drafting provider with scripted replies; records every request so tests
// can prove which paths never reach the AI.
class ScriptedLlmProvider extends FakeLlmProvider {
  requests: GenerateRequestDto[] = [];
  constructor(private readonly replies: ((request: GenerateRequestDto) => string)[]) {
    super(8);
  }
  override async generate(request: GenerateRequestDto) {
    this.requests.push(request);
    const reply = this.replies[Math.min(this.requests.length - 1, this.replies.length - 1)];
    return { text: reply(request), model: 'scripted' };
  }
}

// the passage id of the evidence whose text contains `needle`
const idOf = (request: GenerateRequestDto, needle: string) =>
  request.evidence.find((e) => e.text.includes(needle))?.id ?? 'P1';

const otel = new OpenTelemetryCollector('test', 'error', {});
const embeddings = new FakeLlmProvider(8);
const fetchers = { pmc_oa: new StubFetcher('pmc_oa'), openfda: new StubFetcher('openfda') };

const answerService = async (llm: FakeLlmProvider) => {
  const { SearchService } = await import('../domain/services/search.service');
  const { TopicService } = await import('../domain/services/topic.service');
  const { AnswerService } = await import('../domain/services/answer.service');
  const em = orm.em.fork();
  const search = new SearchService(
    em,
    embeddings,
    new LexicalReranker(),
    new LiveRetrievalService(new SourceFetcherRegistry([]), []),
    otel
  );
  const topics = new TopicService(em, search, otel);
  return { answers: new AnswerService(em, search, topics, llm, otel), topics };
};

const auditRow = async (answerId: string) => {
  const [row] = await orm.em.getConnection().execute(
    `select query, query_class, kind, provider, sentences_kept, sentences_removed, removed_sentences
       from generated_answer where id = ?`,
    [answerId]
  );
  const citations = await orm.em.getConnection().execute(
    `select external_id from answer_citation where answer_id = ? order by external_id`,
    [answerId]
  );
  return { row, citations: (citations as { external_id: string }[]).map((c) => c.external_id) };
};

beforeAll(async () => {
  container = await startPgvectorContainer();
  Object.assign(process.env, {
    DB_NAME: TEST_DB.database,
    DB_HOST: container.getHost(),
    DB_USER: TEST_DB.user,
    DB_PASSWORD: TEST_DB.password,
    DB_PORT: String(container.getMappedPort(5432)),
    NODE_ENV: 'test',
    ENCRYPTION_KEY: '0'.repeat(64)
  });
  orm = await initOrm();
  await orm.migrator.up();

  const { IngestionService } = await import('../domain/services/ingestion.service');
  const registry = new SourceFetcherRegistry(Object.values(fetchers));
  fetchers.pmc_oa.documents = [
    {
      sourceKey: 'pmc_oa',
      externalId: 'PMC1234567',
      title: 'Outcomes of laparoscopic cholecystectomy in a regional centre',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC1234567/',
      publishedAt: '2026-07-03',
      license: 'CC BY 4.0',
      sections: [
        { path: 'Results', text: 'After laparoscopic cholecystectomy the median blood loss was 20 mL and no transfusion was needed.' },
        { path: 'Complications', text: 'Bile leak after laparoscopic cholecystectomy occurred in 2% of patients.' }
      ]
    }
  ];
  await new IngestionService(orm.em.fork(), registry, embeddings, otel).ingest({ sourceKey: 'pmc_oa', term: 'x', limit: 10 });
  fetchers.openfda.documents = [
    {
      sourceKey: 'openfda',
      externalId: 'set-propofol',
      title: 'Propofol injectable emulsion',
      url: 'https://api.fda.gov/drug/label.json?search=set_id:set-propofol',
      license: 'CC0',
      sections: [
        { path: 'Dosage and Administration', text: 'Propofol induction of general anesthesia in healthy adults less than 55 years: 2 to 2.5 mg/kg.' },
        { path: 'Warnings', text: 'Propofol should be administered only by persons trained in general anesthesia.' }
      ]
    }
  ];
  await new IngestionService(orm.em.fork(), registry, embeddings, otel).ingest({ sourceKey: 'openfda', term: 'x', limit: 10 });
}, 180_000);

afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
});

describe('answers on pgvector', () => {
  it('answers a literature question from cited passages and records it', async () => {
    const llm = new ScriptedLlmProvider([
      (r) =>
        `Bile leak after laparoscopic cholecystectomy occurred in 2% of patients. [${idOf(r, 'Bile leak')}]\n` +
        `After laparoscopic cholecystectomy the median blood loss was 20 mL. [${idOf(r, 'blood loss')}]`
    ]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy bile leak blood loss', live: false });

    expect(answer).toMatchObject({ queryClass: 'literature_lookup', kind: 'answer', model: 'scripted' });
    expect(answer.notice).toContain('not for clinical use');
    const [section] = answer.sections;
    expect(section.status).toBe('answered');
    expect(section.sentences.map((s) => s.text)).toEqual([
      'Bile leak after laparoscopic cholecystectomy occurred in 2% of patients.',
      'After laparoscopic cholecystectomy the median blood loss was 20 mL.'
    ]);
    // citations are stored passage ids that resolve to listed sources
    const sourceIds = new Set(answer.sources.map((s) => s.passageId));
    expect(section.sentences.every((s) => s.citations.every((id) => sourceIds.has(id)))).toBe(true);
    // the model saw the source kind for each passage
    expect(llm.requests[0].evidence[0].label).toMatch(/pmc_oa article: Outcomes of laparoscopic cholecystectomy/);

    const { row, citations } = await auditRow(answer.answerId);
    expect(row).toMatchObject({
      query: 'laparoscopic cholecystectomy bile leak blood loss',
      query_class: 'literature_lookup',
      provider: 'fake',
      sentences_kept: 2,
      sentences_removed: 0
    });
    expect(citations).toEqual(['PMC1234567', 'PMC1234567']);
  });

  it('removes invented numbers and citations, then keeps the self-checked draft', async () => {
    const llm = new ScriptedLlmProvider([
      (r) =>
        `After laparoscopic cholecystectomy the median blood loss was 50 mL. [${idOf(r, 'blood loss')}]\n` +
        'Bile leak after laparoscopic cholecystectomy occurred in 2% of patients. [P99]',
      (r) => `After laparoscopic cholecystectomy the median blood loss was 20 mL. [${idOf(r, 'blood loss')}]`
    ]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy blood loss', live: false });

    expect(llm.requests).toHaveLength(2);
    expect(llm.requests[1].prompt).toContain('failed verification');
    expect(answer.sections[0].sentences.map((s) => s.text)).toEqual([
      'After laparoscopic cholecystectomy the median blood loss was 20 mL.'
    ]);
    const { row } = await auditRow(answer.answerId);
    expect(row.sentences_removed).toBe(2);
    expect(row.removed_sentences.map((r: { reason: string }) => r.reason)).toEqual([
      'number 50 is not in the cited passages',
      'cites passages that were not supplied: P99'
    ]);
  });

  it('says insufficient evidence rather than showing unverified text', async () => {
    const llm = new ScriptedLlmProvider([() => 'Robotic surgery is always better. [P1]']);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy blood loss', live: false });
    expect(answer.sections[0]).toMatchObject({ status: 'insufficient_evidence', sentences: [] });
  });

  it('marks a section unavailable when drafting fails', async () => {
    const llm = new ScriptedLlmProvider([
      () => {
        throw new Error('provider down');
      }
    ]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy blood loss', live: false });
    expect(answer.sections[0].status).toBe('generation_failed');
  });

  it('never sends an emergency to the AI and does not store the query', async () => {
    const llm = new ScriptedLlmProvider([() => 'unused']);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'my patient is not breathing right now', live: false });

    expect(llm.requests).toHaveLength(0);
    expect(answer).toMatchObject({ kind: 'emergency', queryClass: 'emergency_pattern', sections: [] });
    expect(answer.message).toContain('emergency');
    const { row } = await auditRow(answer.answerId);
    expect(row).toMatchObject({ query: null, kind: 'emergency', provider: null });
  });

  it('answers a patient-specific dose with the quoted label section, without AI', async () => {
    const llm = new ScriptedLlmProvider([() => 'unused']);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'my patient weighs 80 kg, how much propofol', live: false });

    expect(llm.requests).toHaveLength(0);
    expect(answer.kind).toBe('boundary');
    expect(answer.message).toContain('individual patient');
    expect(answer.sections[0]).toMatchObject({ key: 'label_dosing', status: 'answered' });
    expect(answer.sections[0].sentences[0]).toMatchObject({
      text: 'Propofol induction of general anesthesia in healthy adults less than 55 years: 2 to 2.5 mg/kg.',
      quoted: true
    });
    expect((await auditRow(answer.answerId)).row.query).toBeNull();
  });

  it('refuses prescriptions', async () => {
    const llm = new ScriptedLlmProvider([() => 'unused']);
    const answer = await (await answerService(llm)).answers.answer({ query: 'can you prescribe me propofol', live: false });
    expect(llm.requests).toHaveLength(0);
    expect(answer).toMatchObject({ kind: 'boundary', queryClass: 'prescription_request', sections: [] });
  });

  it('says when a named source is not held', async () => {
    const llm = new ScriptedLlmProvider([() => 'unused']);
    const { answers } = await answerService(llm);
    expect((await answers.answer({ query: 'what did PMID: 99999999 find', live: false })).kind).toBe('source_not_found');
    expect((await answers.answer({ query: 'what did PMC1234567 report on blood loss', live: false })).kind).toBe('answer');
  });

  it('answers each question and phase of a topic page, skipping items without evidence', async () => {
    const llm = new ScriptedLlmProvider([(r) => r.evidence.map((e) => `${e.text} [${e.id}]`).join('\n')]);
    const { answers, topics } = await answerService(llm);
    await topics.assemble('laparoscopic-cholecystectomy');

    const events = [];
    for await (const event of answers.stream({ query: 'laparoscopic cholecystectomy', topicSlug: 'laparoscopic-cholecystectomy' })) {
      events.push(event);
    }
    expect(events[0]).toMatchObject({ type: 'start', kind: 'answer' });
    expect(events.filter((e) => e.type === 'section')).toHaveLength(17);
    const done = events.at(-1);
    if (done?.type !== 'done') throw new Error('no done event');

    const blood = done.answer.sections.find((s) => s.key === 'blood')!;
    expect(blood).toMatchObject({ number: 6, status: 'answered' });
    const positioning = done.answer.sections.find((s) => s.key === 'positioning')!;
    expect(positioning.status).toBe('insufficient_evidence');
    // items without evidence never reach the AI
    expect(llm.requests.length).toBe(done.answer.sections.filter((s) => s.status !== 'insufficient_evidence').length);
    expect(done.answer.notice).toContain('Draft');
  });
});
