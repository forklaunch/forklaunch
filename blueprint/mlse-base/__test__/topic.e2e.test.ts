import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FakeLlmProvider,
  FetchLike,
  ImageSearchService,
  LexicalReranker,
  LiveRetrievalService,
  SourceFetcher,
  SourceFetcherRegistry
} from '@forklaunch/implementation-mlse-base/services';
import { FetchedDocumentDto } from '@forklaunch/interfaces-mlse/types';
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

const llm = new FakeLlmProvider(8);
const otel = new OpenTelemetryCollector('test', 'error', {});
const fetchers = { pmc_oa: new StubFetcher('pmc_oa'), pubmed: new StubFetcher('pubmed') };

const services = async (options: { images?: ImageSearchService } = {}) => {
  const { SearchService } = await import('../domain/services/search.service');
  const { TopicService } = await import('../domain/services/topic.service');
  const em = orm.em.fork();
  const search = new SearchService(
    em,
    llm,
    new LexicalReranker(),
    new LiveRetrievalService(new SourceFetcherRegistry([]), []),
    otel
  );
  // the fake embeddings carry no meaning to fit questions by
  return { topics: new TopicService(em, search, otel, { itemFit: false, ...options }) };
};

const ingest = async (sourceKey: keyof typeof fetchers, documents: FetchedDocumentDto[]) => {
  const { IngestionService } = await import('../domain/services/ingestion.service');
  fetchers[sourceKey].documents = documents;
  await new IngestionService(orm.em.fork(), new SourceFetcherRegistry(Object.values(fetchers)), llm, otel).ingest({
    sourceKey,
    term: 'x',
    limit: 10
  });
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

  await ingest('pmc_oa', [
    {
      sourceKey: 'pmc_oa',
      externalId: 'PMC1',
      title: 'Outcomes of laparoscopic cholecystectomy in a regional centre',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC1/',
      publishedAt: '2026-07-03',
      license: 'CC BY 4.0',
      sections: [
        { path: 'Methods › Surgical technique', text: 'In laparoscopic cholecystectomy the Critical View of Safety was achieved before the cystic duct was clipped and divided.' },
        { path: 'Results', text: 'After laparoscopic cholecystectomy the median blood loss was 20 mL and no transfusion was needed. Of the patients, 60% were female.' },
        { path: 'Complications', text: 'Bile leak after laparoscopic cholecystectomy occurred in 2% of patients.' }
      ]
    },
    {
      // shares "laparoscopic" with the cholecystectomy topic, nothing more
      sourceKey: 'pmc_oa',
      externalId: 'PMC2',
      title: 'Laparoscopic appendectomy for acute appendicitis',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC2/',
      publishedAt: '2026-05-11',
      license: 'CC BY 4.0',
      sections: [
        { path: 'Methods › Operative technique', text: 'In laparoscopic appendectomy the mesoappendix was divided and the appendiceal stump was secured with endoloops.' },
        { path: 'Results', text: 'After laparoscopic appendectomy the median blood loss was 10 mL.' }
      ]
    }
  ]);
  await ingest('pubmed', [
    {
      sourceKey: 'pubmed',
      externalId: 'CASE-VOLVULUS',
      title: 'Surgical management of gallbladder volvulus',
      url: 'https://pubmed.ncbi.nlm.nih.gov/CASE-VOLVULUS/',
      license: 'CC BY 4.0',
      isCaseReport: true,
      meshDescriptorUis: ['D017081', 'D005705'],
      sections: [
        { path: 'Case presentation', text: 'An 80-year-old presented with acute abdominal pain.' },
        { path: 'Management', text: 'Emergency laparoscopic cholecystectomy was performed.' },
        { path: 'Outcome', text: 'She was discharged on day 3.' }
      ]
    },
    {
      sourceKey: 'pubmed',
      externalId: 'CASE-UNRELATED',
      title: 'A rare case of neonatal-onset diarrhea due to congenital tufting enteropathy',
      url: 'https://pubmed.ncbi.nlm.nih.gov/CASE-UNRELATED/',
      license: 'CC BY 4.0',
      isCaseReport: true,
      meshDescriptorUis: ['D003967'],
      // mentions cholecystectomy in passing, which the database search
      // matches, but it is not about the procedure
      sections: [{ path: 'Case presentation', text: 'A neonate with diarrhea; the mother had a laparoscopic cholecystectomy years before.' }]
    },
    {
      sourceKey: 'pubmed',
      externalId: 'CASE-NC',
      title: 'Spilled gallstones after laparoscopic cholecystectomy: a case report',
      url: 'https://pubmed.ncbi.nlm.nih.gov/CASE-NC/',
      license: 'CC BY-NC-ND 4.0',
      isCaseReport: true,
      sections: [{ path: 'Case presentation', text: 'Text that the license does not allow MLSE to store.' }]
    },
    {
      sourceKey: 'pubmed',
      externalId: 'CASE-RETRACTED',
      title: 'Retracted laparoscopic cholecystectomy case',
      url: 'https://pubmed.ncbi.nlm.nih.gov/CASE-RETRACTED/',
      license: 'CC BY 4.0',
      isCaseReport: true,
      retracted: true,
      meshDescriptorUis: ['D017081'],
      sections: [{ path: 'Case presentation', text: 'Withdrawn.' }]
    }
  ]);

  const { IngestionService } = await import('../domain/services/ingestion.service');
  await new IngestionService(orm.em.fork(), new SourceFetcherRegistry([]), llm, otel).loadMeshConcepts([
    { descriptorUi: 'D017081', preferredTerm: 'Cholecystectomy, Laparoscopic', synonyms: ['Laparoscopic Cholecystectomy'], treeNumbers: [] },
    { descriptorUi: 'D005705', preferredTerm: 'Gallbladder Diseases', synonyms: [], treeNumbers: [] }
  ]);
}, 180_000);

// stopping the database container can take a while on a busy machine
afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
}, 60_000);

describe('topic pages on pgvector', () => {
  it('seeds the procedure pages as drafts', async () => {
    const { topics } = await services();
    const list = await topics.listTopics();
    expect(list.map((topic) => topic.slug)).toEqual([
      'appendectomy',
      'cesarean-section',
      'coronary-artery-bypass-grafting',
      'inguinal-hernia-repair',
      'laparoscopic-cholecystectomy',
      'total-knee-arthroplasty'
    ]);
    expect(list.every((topic) => topic.topicType === 'procedure' && topic.status === 'draft')).toBe(true);
  });

  it('assembles an evidence map and reports the gaps', async () => {
    const { topics } = await services();
    const result = await topics.assemble('laparoscopic-cholecystectomy');
    expect(result.items).toBe(17);
    expect(result.withEvidence).toBeGreaterThan(0);
    // nothing in the corpus covers positioning
    expect(result.insufficientEvidence).toContain('positioning');
    expect(result.caseStudies).toBe(2);
    expect(result.casesExcluded).toBe(1);
  });

  it('links phases to the passages that address them', async () => {
    const page = await (await services()).topics.getPage('laparoscopic-cholecystectomy');
    expect(page.status).toBe('draft');
    expect(page.notice).toContain('not for clinical use');

    const blood = page.phases.find((p) => p.key === 'blood')!;
    expect(blood.status).toBe('evidence_found');
    expect(blood.evidence[0]).toMatchObject({ externalId: 'PMC1', sectionPath: 'Results' });

    const core = page.phases.find((p) => p.key === 'core')!;
    expect(core.evidence.map((e: { sectionPath: string }) => e.sectionPath)).toContain('Methods › Surgical technique');

    const positioning = page.phases.find((p) => p.key === 'positioning')!;
    expect(positioning).toMatchObject({ status: 'insufficient_evidence', evidence: [] });

    const risks = page.questions.find((q) => q.key === 'risks')!;
    expect(risks.evidence.map((e: { sectionPath: string }) => e.sectionPath)).toContain('Complications');
  });

  it('extracts numbers from quantitative items for review only', async () => {
    const page = await (await services()).topics.getPage('laparoscopic-cholecystectomy');
    const blood = page.phases.find((p) => p.key === 'blood')!;
    expect(blood.facts).toEqual([
      expect.objectContaining({ raw: '20 mL', low: 20, high: 20, unit: 'mL', statistic: 'median', reviewStatus: 'unreviewed' })
    ]);
  });

  it('groups relevant case studies by diagnosis within license limits', async () => {
    const page = await (await services()).topics.getPage('laparoscopic-cholecystectomy');
    const byDiagnosis = Object.fromEntries(page.caseStudies.map((g) => [g.diagnosis, g]));

    const volvulus = byDiagnosis['Gallbladder Diseases'].cases[0] as Record<string, unknown>;
    expect(volvulus).toMatchObject({
      externalId: 'CASE-VOLVULUS',
      relevanceReason: 'mesh',
      presentation: 'An 80-year-old presented with acute abdominal pain.',
      management: 'Emergency laparoscopic cholecystectomy was performed.',
      outcome: 'She was discharged on day 3.'
    });

    // non-commercial license: listed with a link, but no stored text
    const nc = byDiagnosis['Unclassified'].cases[0] as Record<string, unknown>;
    expect(nc).toMatchObject({ externalId: 'CASE-NC', licenseScope: 'metadata_only', relevanceReason: 'terms' });
    expect(nc.presentation).toBeUndefined();

    const all = page.caseStudies.flatMap((g) => g.cases as { externalId: string }[]).map((c) => c.externalId);
    expect(all).not.toContain('CASE-UNRELATED');
    expect(all).not.toContain('CASE-RETRACTED');
    expect(page.caseStudies.every((g) => g.evidenceLevel === 'case report (low)')).toBe(true);
  });

  it("builds each procedure from its own papers and its own words", async () => {
    const { topics } = await services();
    await topics.assemble('appendectomy');
    const appendectomy = await topics.getPage('appendectomy');
    // "mesoappendix" and "stump" are the appendectomy's words, not the framework's
    const core = appendectomy.phases.find((p) => p.key === 'core')!;
    expect(core.evidence.map((e: { externalId: string }) => e.externalId)).toEqual(['PMC2']);

    // the appendectomy paper never reaches the cholecystectomy page
    const cholecystectomy = await topics.getPage('laparoscopic-cholecystectomy');
    const cited = [...cholecystectomy.questions, ...cholecystectomy.phases].flatMap((item) =>
      item.evidence.map((e: { externalId: string }) => e.externalId)
    );
    expect(cited).not.toContain('PMC2');
  });

  it("shows figures of a procedure's steps, each under one step", async () => {
    // Open-i answers every query with the same figures
    const figure = (pmcid: number, caption: string, license = 'https://creativecommons.org/licenses/by/4.0/') => ({
      uid: `PMC${pmcid}`,
      pmcid: String(pmcid),
      title: 'Two-port laparoscopic appendectomy',
      licenseURL: license,
      imgLarge: `/imgs/512/1/${pmcid}.png`,
      image: { id: 'F1', caption, modalityMajor: 'ph' }
    });
    const openI: FetchLike = async () => ({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify({
          list: [
            figure(11, 'First umbilical port placed for the camera'),
            figure(12, 'Endoloop placed on the appendiceal stump'),
            figure(13, 'Umbilical incision closed', 'https://creativecommons.org/licenses/by-nc/4.0/'),
            figure(14, 'Ultrasound of the inflamed appendix')
          ]
        })
    });
    const { topics } = await services({ images: new ImageSearchService(openI) });

    const result = await topics.assemble('appendectomy');
    const page = await topics.getPage('appendectomy');
    const shown = (key: string) =>
      [...page.questions, ...page.phases].find((item) => item.key === key)!.figures.map((f: { pmcid: string }) => f.pmcid);

    expect(shown('access')).toEqual(['PMC11']);
    expect(shown('core')).toEqual(['PMC12']);
    // already shown under a phase; the NonCommercial and ultrasound figures never
    expect(shown('how')).toEqual([]);
    expect(result.figures).toBe(2);
  });

  it('returns a single phase for jumping straight to it', async () => {
    const page = await (await services()).topics.getPage('laparoscopic-cholecystectomy', 6);
    expect(page.phases.map((p) => p.key)).toEqual(['blood']);
    expect(page.questions).toEqual([]);
  });

  it('reports an unknown topic', async () => {
    await expect((await services()).topics.getPage('no-such-topic')).rejects.toThrow("Topic 'no-such-topic' not found");
  });
});
