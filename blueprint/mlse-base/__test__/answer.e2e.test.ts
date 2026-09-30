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

const NEWLINE = String.fromCharCode(10);
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

// stopping the database container can take a while on a busy machine
afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
}, 60_000);

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

  it('quotes the sources instead of showing unverified AI text', async () => {
    const llm = new ScriptedLlmProvider([() => 'Robotic surgery is always better. [P1]']);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy blood loss', live: false });
    const [section] = answer.sections;
    expect(section.status).toBe('quoted_evidence');
    expect(section.sentences.map((s) => s.text)).not.toContain('Robotic surgery is always better.');
    // every quote is verbatim from the passage it cites
    for (const sentence of section.sentences) {
      expect(sentence.quoted).toBe(true);
      const source = answer.sources.find((p) => p.passageId === sentence.citations[0])!;
      expect(source.text).toContain(sentence.text);
    }
  });

  it('says insufficient evidence when no source is about the question', async () => {
    const llm = new ScriptedLlmProvider([() => 'Anything. [P1]']);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'heart attack', live: false });
    expect(llm.requests).toHaveLength(0);
    expect(answer.sections[0]).toMatchObject({ status: 'insufficient_evidence', sentences: [] });
    expect(answer.research).toMatchObject({ searchedFor: ['heart attack', 'myocardial infarction', 'acute coronary syndrome'], aboutQuestion: 0, used: 0 });
  });

  it('quotes the sources when drafting fails', async () => {
    const llm = new ScriptedLlmProvider([
      () => {
        throw new Error('provider down');
      }
    ]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy blood loss', live: false });
    // the sources are still quoted when the AI is down
    expect(answer.sections[0].status).toBe('quoted_evidence');
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

  it('gives a procedure an overview, section by section', async () => {
    const llm = new ScriptedLlmProvider([(r) => r.evidence.map((e) => `${e.text} [${e.id}]`).join(NEWLINE)]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'laparoscopic cholecystectomy', live: false });

    expect(answer.research).toMatchObject({ topicType: 'procedure' });
    expect(answer.sections.map((s) => s.key)).toEqual(['what', 'how', 'risks', 'recovery']);
    const risks = answer.sections.find((s) => s.key === 'risks')!;
    expect(risks.status).toBe('answered');
    expect(risks.sentences[0].text).toContain('Bile leak');
    // sections with no passage addressing them never reach the AI
    expect(llm.requests.length).toBe(answer.sections.filter((s) => s.status !== 'insufficient_evidence').length);
  });

  it('ends a medicine overview with the label dosing quoted, never AI-written', async () => {
    const llm = new ScriptedLlmProvider([(r) => r.evidence.map((e) => `${e.text} [${e.id}]`).join(NEWLINE)]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'propofol', live: false });

    expect(answer.research).toMatchObject({ topicType: 'medication' });
    const dosing = answer.sections.at(-1)!;
    expect(dosing).toMatchObject({ key: 'label_dosing', status: 'answered' });
    expect(dosing.sentences.every((s) => s.quoted)).toBe(true);
    expect(llm.requests.every((r) => !r.prompt.toLowerCase().includes('dosing'))).toBe(true);
  });

  it('answers a follow-up directly, in the context of its topic', async () => {
    const llm = new ScriptedLlmProvider([(r) => r.evidence.map((e) => `${e.text} [${e.id}]`).join(NEWLINE)]);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'what about bile leak?', followUpOf: 'laparoscopic cholecystectomy', live: false });

    expect(answer.query).toBe('laparoscopic cholecystectomy: what about bile leak?');
    expect(answer.sections).toHaveLength(1);
    expect(answer.sections[0].sentences.map((s) => s.text).join(' ')).toContain('Bile leak');
  });

  it('keeps the safety rules for a follow-up about one patient', async () => {
    const llm = new ScriptedLlmProvider([() => 'unused']);
    const { answers } = await answerService(llm);
    const answer = await answers.answer({ query: 'how much should I give my patient, he weighs 80 kg?', followUpOf: 'propofol', live: false });

    expect(llm.requests).toHaveLength(0);
    expect(answer).toMatchObject({ kind: 'boundary', queryClass: 'patient_specific_treatment' });
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
