import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FakeLlmProvider,
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
    migrations: { path: path.join(__dirname, '../migrations'), glob: '!(*.d).{js,ts}' }
  });
};

let container: StartedTestContainer;
let orm: Awaited<ReturnType<typeof initOrm>>;

class StubFetcher implements SourceFetcher {
  documents: FetchedDocumentDto[] = [];
  delayMs = 0;
  constructor(readonly sourceKey: string) {}
  async fetchDocuments() {
    if (this.delayMs) {
      await new Promise((r) => setTimeout(r, this.delayMs));
    }
    return this.documents;
  }
}

const llm = new FakeLlmProvider(8);
const otel = new OpenTelemetryCollector('test', 'error', {});
const corpus = {
  openfda: new StubFetcher('openfda'),
  pmc_oa: new StubFetcher('pmc_oa'),
  pubmed: new StubFetcher('pubmed')
};
const live = {
  pubmed: new StubFetcher('pubmed'),
  clinicaltrials: new StubFetcher('clinicaltrials')
};

const ingest = async (sourceKey: keyof typeof corpus, documents: FetchedDocumentDto[]) => {
  const { IngestionService } = await import('../domain/services/ingestion.service');
  corpus[sourceKey].documents = documents;
  await new IngestionService(
    orm.em.fork(),
    new SourceFetcherRegistry(Object.values(corpus)),
    llm,
    otel
  ).ingest({ sourceKey, term: 'x', limit: 10 });
};

const searchService = async () => {
  const { SearchService } = await import('../domain/services/search.service');
  return new SearchService(
    orm.em.fork(),
    llm,
    new LexicalReranker(),
    new LiveRetrievalService(
      new SourceFetcherRegistry(Object.values(live)),
      ['pubmed', 'clinicaltrials'],
      undefined,
      { timeoutMs: 200 }
    ),
    otel
  );
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

  await ingest('openfda', [
    {
      sourceKey: 'openfda',
      externalId: 'set-cefazolin',
      title: 'Cefazolin — Example Labs',
      url: 'https://dailymed.nlm.nih.gov/dailymed/lookup.cfm?setid=set-cefazolin',
      publishedAt: '2026-01-07',
      license: 'CC0',
      sections: [
        { path: 'Indications and Usage', text: 'Perioperative prophylaxis: cefazolin reduces postoperative infection in contaminated surgery.' },
        { path: 'Adverse Reactions', text: 'Diarrhea and rash have been reported.' }
      ]
    }
  ]);
  await ingest('pmc_oa', [
    {
      sourceKey: 'pmc_oa',
      externalId: 'PMC1',
      title: 'Outcomes of laparoscopic cholecystectomy with common bile duct exploration',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC1/',
      publishedAt: '2026-07-03',
      license: 'CC BY 4.0',
      sections: [{ path: 'Results', text: 'After laparoscopic cholecystectomy the median blood loss was 20 mL.' }]
    },
    {
      sourceKey: 'pmc_oa',
      externalId: 'PMC-RETRACTED',
      title: 'A retracted cholecystectomy trial',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC-RETRACTED/',
      license: 'CC BY 4.0',
      retracted: true,
      sections: [{ path: 'Results', text: 'Laparoscopic cholecystectomy blood loss was implausibly low.' }]
    }
  ]);
  await ingest('pubmed', [
    {
      sourceKey: 'pubmed',
      externalId: '42785763',
      title: 'Surgical management of gallbladder volvulus',
      url: 'https://pubmed.ncbi.nlm.nih.gov/42785763/',
      publishedAt: '2026-09-20',
      license: 'publisher-copyright-abstract',
      isCaseReport: true,
      sections: [{ path: 'Abstract', text: 'Gallbladder volvulus treated by emergency laparoscopic cholecystectomy.' }]
    }
  ]);
  // an older version that must never be returned
  await ingest('openfda', [
    {
      sourceKey: 'openfda',
      externalId: 'set-cefazolin',
      title: 'Cefazolin — Example Labs',
      url: 'https://dailymed.nlm.nih.gov/dailymed/lookup.cfm?setid=set-cefazolin',
      publishedAt: '2026-02-01',
      license: 'CC0',
      sections: [
        { path: 'Indications and Usage', text: 'Perioperative prophylaxis: cefazolin reduces postoperative infection in contaminated and clean-contaminated surgery.' },
        { path: 'Adverse Reactions', text: 'Diarrhea and rash have been reported.' }
      ]
    }
  ]);

  const { IngestionService } = await import('../domain/services/ingestion.service');
  await new IngestionService(orm.em.fork(), new SourceFetcherRegistry([]), llm, otel).loadMeshConcepts([
    {
      descriptorUi: 'D017081',
      preferredTerm: 'Cholecystectomy, Laparoscopic',
      synonyms: ['Laparoscopic Cholecystectomy', 'Celioscopic Cholecystectomy'],
      treeNumbers: ['E04.210.240.130.500']
    }
  ]);
}, 180_000);

afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
});

describe('hybrid search on pgvector', () => {
  it('finds passages by keyword and cites their document', async () => {
    const { results } = await (await searchService()).search({ query: 'perioperative prophylaxis', live: false });
    expect(results[0]).toMatchObject({
      origin: 'corpus',
      sourceKey: 'openfda',
      externalId: 'set-cefazolin',
      sectionPath: 'Indications and Usage'
    });
    expect(results[0].matchedBy).toContain('keyword');
  });

  it('only returns the current version of a document', async () => {
    const { results } = await (await searchService()).search({ query: 'cefazolin', live: false });
    const texts = results.filter((r) => r.sectionPath === 'Indications and Usage').map((r) => r.text);
    expect(texts).toEqual([
      'Perioperative prophylaxis: cefazolin reduces postoperative infection in contaminated and clean-contaminated surgery.'
    ]);
  });

  it('never returns retracted documents', async () => {
    const { results } = await (await searchService()).search({ query: 'laparoscopic cholecystectomy blood loss', live: false });
    expect(results.map((r) => r.externalId)).not.toContain('PMC-RETRACTED');
    expect(results.map((r) => r.externalId)).toContain('PMC1');
  });

  it('expands the query with MeSH synonyms', async () => {
    const response = await (await searchService()).search({ query: 'celioscopic cholecystectomy', live: false });
    expect(response.expandedTerms).toEqual(
      expect.arrayContaining(['Cholecystectomy, Laparoscopic', 'Laparoscopic Cholecystectomy'])
    );
    // no stored text says "celioscopic"; the synonym finds it anyway
    expect(response.results.map((r) => r.externalId)).toContain('PMC1');
  });

  it('matches by meaning through the vector index', async () => {
    const { results } = await (await searchService()).search({
      query: 'Adverse Reactions: Diarrhea and rash have been reported.',
      live: false
    });
    const top = results[0];
    expect(top.sectionPath).toBe('Adverse Reactions');
    expect(top.matchedBy).toContain('vector');
  });

  it('filters to case reports, sources and dates', async () => {
    const service = await searchService();
    const caseReports = await service.search({ query: 'laparoscopic cholecystectomy', caseReportsOnly: true, live: false });
    expect(caseReports.results.length).toBeGreaterThan(0);
    expect(caseReports.results.every((r) => r.isCaseReport)).toBe(true);

    const onlyPmc = await service.search({ query: 'laparoscopic cholecystectomy', sourceKeys: ['pmc_oa'], live: false });
    expect(new Set(onlyPmc.results.map((r) => r.sourceKey))).toEqual(new Set(['pmc_oa']));

    const recent = await service.search({ query: 'laparoscopic cholecystectomy', publishedAfter: '2026-09', live: false });
    expect(recent.results.map((r) => r.externalId)).toEqual(['42785763']);
  });

  it('merges live results, skips ones already stored, and reports slow sources', async () => {
    live.pubmed.documents = [
      {
        sourceKey: 'pubmed',
        externalId: 'NEW-1',
        title: 'Bile leak after laparoscopic cholecystectomy',
        url: 'https://pubmed.ncbi.nlm.nih.gov/NEW-1/',
        license: 'CC BY 4.0',
        sections: [{ path: 'Abstract', text: 'Bile leak after laparoscopic cholecystectomy was managed endoscopically.' }]
      },
      {
        // already in the corpus: must not appear twice
        sourceKey: 'pubmed',
        externalId: '42785763',
        title: 'Surgical management of gallbladder volvulus',
        url: 'https://pubmed.ncbi.nlm.nih.gov/42785763/',
        license: 'CC BY 4.0',
        sections: [{ path: 'Abstract', text: 'Gallbladder volvulus treated by emergency laparoscopic cholecystectomy.' }]
      }
    ];
    live.clinicaltrials.delayMs = 1000;

    const response = await (await searchService()).search({ query: 'bile leak laparoscopic cholecystectomy' });
    const liveResults = response.results.filter((r) => r.origin === 'live');
    expect(liveResults.map((r) => r.externalId)).toEqual(['NEW-1']);
    expect(liveResults[0].matchedBy).toEqual(['live']);
    expect(response.results.filter((r) => r.externalId === '42785763').every((r) => r.origin === 'corpus')).toBe(true);
    expect(response.liveSources).toEqual([
      { sourceKey: 'pubmed', status: 'ok', documents: 2 },
      { sourceKey: 'clinicaltrials', status: 'timeout', documents: 0, error: undefined }
    ]);
  });
});
