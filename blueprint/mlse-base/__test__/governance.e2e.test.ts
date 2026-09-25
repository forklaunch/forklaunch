import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FakeLlmProvider,
  LexicalReranker,
  LicensedContentAdapter,
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

const ORG_A = 'org-a';
const ORG_B = 'org-b';

const otel = new OpenTelemetryCollector('test', 'error', {});
const llm = new FakeLlmProvider(8);
const pmc = new StubFetcher('pmc_oa');
const guidelineFeed = new StubFetcher('guideline_feed');
const registry = new SourceFetcherRegistry([pmc, new LicensedContentAdapter(guidelineFeed, { scope: 'full_text' })]);

const services = async () => {
  const { SearchService } = await import('../domain/services/search.service');
  const { TopicService } = await import('../domain/services/topic.service');
  const { AnswerService } = await import('../domain/services/answer.service');
  const { VoiceService } = await import('../domain/services/voice.service');
  const { GovernanceService } = await import('../domain/services/governance.service');
  const { SavedSearchService } = await import('../domain/services/savedSearch.service');
  const { IngestionService } = await import('../domain/services/ingestion.service');
  const em = orm.em.fork();
  const search = new SearchService(em, llm, new LexicalReranker(), new LiveRetrievalService(new SourceFetcherRegistry([]), []), otel);
  const answers = new AnswerService(em, search, new TopicService(em, search, otel), llm, otel);
  return {
    search,
    answers,
    voice: new VoiceService(em, answers, otel),
    governance: new GovernanceService(em, otel),
    saved: new SavedSearchService(em),
    ingestion: new IngestionService(em, registry, llm, otel)
  };
};

const externalIds = async (organizationId: string | undefined, query: string) => {
  const { search } = await services();
  const { results } = await search.search({ query, live: false, ...(organizationId ? { organizationId } : {}) });
  return [...new Set(results.map((r) => r.externalId))].sort();
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

  pmc.documents = [
    {
      sourceKey: 'pmc_oa',
      externalId: 'PMC1234567',
      title: 'Outcomes of laparoscopic cholecystectomy in a regional centre',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC1234567/',
      license: 'CC BY 4.0',
      sections: [{ path: 'Results', text: 'After laparoscopic cholecystectomy the median blood loss was 20 mL.' }]
    }
  ];
  const { ingestion } = await services();
  // the worker registers the public sources at startup
  await ingestion.syncSources([
    { id: 'pmc_oa', name: 'PMC Open Access', tier: 'literature_full_text', licenseTerms: 'CC BY', commercialUse: true, liveQuery: true }
  ]);
  await ingestion.ingest({ sourceKey: 'pmc_oa', term: 'x', limit: 10 });
  guidelineFeed.documents = [
    {
      sourceKey: 'guideline_feed',
      externalId: 'GL-1',
      title: 'Society guideline on laparoscopic cholecystectomy',
      url: 'https://guidelines.example.org/gl-1',
      license: 'All rights reserved',
      sections: [{ path: 'Recommendations', text: 'Laparoscopic cholecystectomy blood loss guideline recommendation.' }]
    }
  ];
}, 180_000);

afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
});

describe('voice settings', () => {
  it('is off by default and refuses the request', async () => {
    const { voice } = await services();
    await expect(
      voice.query({ organizationId: ORG_A, area: 'operating_room', transcript: 'blood loss laparoscopic cholecystectomy' })
    ).rejects.toMatchObject({ name: 'VoiceDisabledError' });
  });

  it('answers only in the area that enabled it, for that organization only', async () => {
    const { voice } = await services();
    await voice.setSetting(ORG_A, 'operating_room', true, 'chief-of-surgery');
    await voice.setSetting(ORG_A, 'emergency', false, 'ed-lead');

    const result = await voice.query({
      organizationId: ORG_A,
      area: 'operating_room',
      userId: 'surgeon-1',
      transcript: 'Mrs Anne Jones, MRN 4471923: blood loss in laparoscopic cholecystectomy?'
    });
    expect(result.identifiersRemoved).toBe(2);
    expect(result.answer.query).not.toMatch(/Anne|Jones|4471923/);
    expect(result.spokenSummary).toMatch(/on screen\.$/);

    await expect(
      voice.query({ organizationId: ORG_A, area: 'emergency', transcript: 'blood loss' })
    ).rejects.toMatchObject({ name: 'VoiceDisabledError' });
    // organization B never enabled voice
    await expect(
      voice.query({ organizationId: ORG_B, area: 'operating_room', transcript: 'blood loss' })
    ).rejects.toMatchObject({ name: 'VoiceDisabledError' });
    expect(await voice.listSettings(ORG_B)).toEqual([]);
  });

  it('rejects malformed care areas', async () => {
    const { voice } = await services();
    await expect(voice.setSetting(ORG_A, 'Operating Room', true, 'x')).rejects.toMatchObject({
      name: 'InvalidCareAreaError'
    });
  });
});

describe('licensed content', () => {
  it('refuses to ingest a licensed source nobody holds a license for', async () => {
    const { ingestion, governance } = await services();
    // not even registered yet: the adapter alone marks it as licensed
    await expect(ingestion.ingest({ sourceKey: 'guideline_feed', term: 'x', limit: 5 })).rejects.toMatchObject({
      name: 'LicenseRequiredError'
    });
    await governance.registerLicensedSource({
      sourceKey: 'guideline_feed',
      name: 'Society guidelines',
      tier: 'guideline',
      licenseTerms: 'Institutional subscription',
      commercialUse: true
    });
    await expect(ingestion.ingest({ sourceKey: 'guideline_feed', term: 'x', limit: 5 })).rejects.toMatchObject({
      name: 'LicenseRequiredError'
    });
  });

  it('shows licensed content only to the organization holding an active license', async () => {
    const { ingestion, governance } = await services();
    const license = await governance.createLicense({
      organizationId: ORG_A,
      sourceKey: 'guideline_feed',
      licensee: 'Hospital A',
      createdBy: 'admin'
    });
    await ingestion.ingest({ sourceKey: 'guideline_feed', term: 'x', limit: 5 });

    const query = 'laparoscopic cholecystectomy blood loss';
    expect(await externalIds(ORG_A, query)).toEqual(['GL-1', 'PMC1234567']);
    expect(await externalIds(ORG_B, query)).toEqual(['PMC1234567']);
    expect(await externalIds(undefined, query)).toEqual(['PMC1234567']);
    expect((await governance.sourceAccess(ORG_B)).find((s) => s.sourceKey === 'guideline_feed')).toMatchObject({
      requiresLicense: true,
      accessible: false
    });

    await governance.revokeLicense(ORG_A, license.id);
    expect(await externalIds(ORG_A, query)).toEqual(['PMC1234567']);
    // another organization cannot revoke it
    await expect(governance.revokeLicense(ORG_B, license.id)).rejects.toMatchObject({ name: 'GovernanceNotFoundError' });
  });

  it('refuses a license for a public source', async () => {
    const { governance } = await services();
    await expect(
      governance.createLicense({ organizationId: ORG_A, sourceKey: 'pmc_oa', licensee: 'x', createdBy: 'x' })
    ).rejects.toMatchObject({ name: 'GovernanceConflictError' });
  });
});

describe('content flags', () => {
  it('hides a flagged document until a reviewer decides', async () => {
    const { governance } = await services();
    const flag = await governance.flag({
      sourceKey: 'pmc_oa',
      externalId: 'PMC1234567',
      reason: 'blood loss figure misreported',
      flaggedBy: 'reviewer-1'
    });
    expect(await externalIds(undefined, 'laparoscopic cholecystectomy blood loss')).toEqual([]);
    expect((await governance.listFlags('open')).map((f) => f.id)).toEqual([flag.id]);

    await governance.resolveFlag(flag.id, 'rejected', 'reviewer-2', 'figure matches the source');
    expect(await externalIds(undefined, 'laparoscopic cholecystectomy blood loss')).toEqual(['PMC1234567']);
    await expect(governance.resolveFlag(flag.id, 'resolved', 'x', 'x')).rejects.toMatchObject({
      name: 'GovernanceConflictError'
    });
  });
});

describe('topic approval', () => {
  it('is refused while the question framework is a draft', async () => {
    const { governance } = await services();
    await expect(governance.approveTopic('laparoscopic-cholecystectomy', 'dr-x')).rejects.toMatchObject({
      name: 'GovernanceConflictError'
    });
  });
});

describe('saved searches and history', () => {
  it('keeps each organization and user separate, encrypted at rest', async () => {
    const { saved } = await services();
    await saved.save(ORG_A, 'user-1', 'Gallbladder', 'laparoscopic cholecystectomy blood loss');
    expect((await saved.list(ORG_A, 'user-1')).map((s) => s.query)).toEqual(['laparoscopic cholecystectomy blood loss']);
    expect(await saved.list(ORG_B, 'user-1')).toEqual([]);
    expect(await saved.list(ORG_A, 'user-2')).toEqual([]);

    const [raw] = await orm.em.getConnection().execute(`select query from saved_search limit 1`);
    expect(raw.query).not.toContain('cholecystectomy');
  });

  it('records history without the text of patient-specific queries', async () => {
    const { answers, saved } = await services();
    await answers.answer({ query: 'laparoscopic cholecystectomy blood loss', live: false, organizationId: ORG_A, userId: 'user-3' });
    await answers.answer({ query: 'my patient weighs 80 kg, how much propofol', live: false, organizationId: ORG_A, userId: 'user-3' });

    const history = await saved.history(ORG_A, 'user-3');
    expect(history.map((h) => [h.queryClass, h.query ?? null]).sort()).toEqual([
      ['literature_lookup', 'laparoscopic cholecystectomy blood loss'],
      ['patient_specific_treatment', null]
    ]);
    expect(await saved.history(ORG_B, 'user-3')).toEqual([]);
  });

  it('anonymizes history older than 90 days', async () => {
    const { saved } = await services();
    const { anonymizeExpiredHistory } = await import('../domain/historyRetention');
    await orm.em.getConnection().execute(
      `update search_history set created_at = now() - interval '100 days' where user_id = 'user-3'`
    );
    // both of user-3's entries are older than 90 days
    expect(await anonymizeExpiredHistory(orm.em.fork())).toBe(2);
    expect((await saved.history(ORG_A, 'user-3')).every((h) => h.query === undefined)).toBe(true);
    // recent history is untouched
    expect((await saved.history(ORG_A, 'surgeon-1')).some((h) => h.query !== undefined)).toBe(true);
  });

  it('exports and erases the saved searches and history of a user', async () => {
    const { saved } = await services();
    const exported = await saved.exportUser('surgeon-1');
    expect(exported.SearchHistory).toHaveLength(1);
    expect((await saved.exportUser('user-1')).SavedSearch.map((s) => s.query)).toEqual([
      'laparoscopic cholecystectomy blood loss'
    ]);

    expect(await saved.eraseUser('user-1')).toEqual({ entitiesAffected: ['SavedSearch'], recordsDeleted: 1 });
    expect(await saved.list(ORG_A, 'user-1')).toEqual([]);
    expect(await saved.eraseUser('user-1')).toEqual({ entitiesAffected: [], recordsDeleted: 0 });
  });
});
