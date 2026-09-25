import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FakeLlmProvider,
  MeshDescriptorParser,
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
    migrations: {
      path: path.join(__dirname, '../migrations'),
      glob: '!(*.d).{js,ts}'
    }
  });
};

let container: StartedTestContainer;
let orm: Awaited<ReturnType<typeof initOrm>>;

// A fetcher whose documents each test sets directly.
class StubFetcher implements SourceFetcher {
  documents: FetchedDocumentDto[] = [];
  constructor(readonly sourceKey: string) {}
  async fetchDocuments() {
    return this.documents;
  }
}

const openfda = new StubFetcher('openfda');
const pubmed = new StubFetcher('pubmed');
const pmc = new StubFetcher('pmc_oa');
const llm = new FakeLlmProvider(8);
const otel = new OpenTelemetryCollector('test', 'error', {});

const cefazolin = (dosing: string): FetchedDocumentDto => ({
  sourceKey: 'openfda',
  externalId: 'set-cefazolin',
  title: 'Cefazolin — Example Labs',
  url: 'https://dailymed.nlm.nih.gov/dailymed/lookup.cfm?setid=set-cefazolin',
  publishedAt: '2026-01-07',
  license: 'CC0',
  sections: [
    { path: 'Indications and Usage', text: 'Cefazolin is indicated for perioperative prophylaxis.' },
    { path: 'Dosage and Administration', text: dosing }
  ]
});

const service = async () => {
  const { IngestionService } = await import('../domain/services/ingestion.service');
  return new IngestionService(
    orm.em.fork(),
    new SourceFetcherRegistry([openfda, pubmed, pmc]),
    llm,
    otel,
    { excerptChars: 80 }
  );
};

const entities = async () => {
  const [{ Document }, { DocumentChunk }, { MedicalConcept }, { Source }] = await Promise.all([
    import('../persistence/entities/document.entity'),
    import('../persistence/entities/documentChunk.entity'),
    import('../persistence/entities/medicalConcept.entity'),
    import('../persistence/entities/source.entity')
  ]);
  return { Document, DocumentChunk, MedicalConcept, Source };
};

beforeAll(async () => {
  container = await startPgvectorContainer();
  process.env.DB_NAME = TEST_DB.database;
  process.env.DB_HOST = container.getHost();
  process.env.DB_USER = TEST_DB.user;
  process.env.DB_PASSWORD = TEST_DB.password;
  process.env.DB_PORT = String(container.getMappedPort(5432));
  process.env.NODE_ENV = 'test';
  process.env.ENCRYPTION_KEY = '0'.repeat(64);
  orm = await initOrm();
  await orm.migrator.up();

  const ingestion = await service();
  await ingestion.syncSources([
    { id: 'openfda', name: 'openFDA drug labels', tier: 'regulatory', licenseTerms: 'CC0', commercialUse: true, liveQuery: true }
  ]);
}, 180_000);

afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
});

describe('corpus ingestion on pgvector', () => {
  it('stores full-text documents as embedded, searchable passages', async () => {
    openfda.documents = [cefazolin('Adults: 2 g IV within 60 minutes before incision.')];
    const result = await (await service()).ingest({ sourceKey: 'openfda', term: 'cefazolin', limit: 5 });

    expect(result).toMatchObject({ fetched: 1, created: 1, updated: 0, unchanged: 0, passages: 2 });

    const { Document, DocumentChunk, Source } = await entities();
    const em = orm.em.fork();
    const doc = await em.findOneOrFail(Document, { externalId: 'set-cefazolin' });
    expect(doc).toMatchObject({ version: 1, status: 'current', licenseScope: 'full_text' });

    const chunks = await em.find(DocumentChunk, { document: doc.id }, { orderBy: { ordinal: 'asc' } });
    expect(chunks.map((c) => c.sectionPath)).toEqual(['Indications and Usage', 'Dosage and Administration']);
    expect(chunks[0].embedding).toHaveLength(8);
    expect(chunks[0].embeddingModel).toBe('fake-embedding');

    // PostgreSQL-generated full-text column, stemmed ('incision' ~ 'incisions')
    const hits = await em.getConnection().execute<{ section_path: string }[]>(
      `select section_path from document_chunk where search_vector @@ plainto_tsquery('english', 'incisions')`
    );
    expect(hits.map((h) => h.section_path)).toEqual(['Dosage and Administration']);

    // nearest neighbour by embedding
    const [query] = (await llm.embed({ texts: ['Dosage and Administration: Adults: 2 g IV within 60 minutes before incision.'] })).embeddings;
    const nearest = await em.getConnection().execute<{ section_path: string }[]>(
      `select section_path from document_chunk order by embedding <-> ?::vector limit 1`,
      [`[${query.join(',')}]`]
    );
    expect(nearest[0].section_path).toBe('Dosage and Administration');

    const source = await em.findOneOrFail(Source, { sourceKey: 'openfda' });
    expect(source.lastRefreshedAt).toBeInstanceOf(Date);
  });

  it('skips an unchanged document on re-ingestion', async () => {
    openfda.documents = [cefazolin('Adults: 2 g IV within 60 minutes before incision.')];
    const result = await (await service()).ingest({ sourceKey: 'openfda', term: 'cefazolin', limit: 5 });
    expect(result).toMatchObject({ unchanged: 1, created: 0, updated: 0, passages: 0 });
  });

  it('versions a changed document and supersedes the old one', async () => {
    openfda.documents = [cefazolin('Adults: 2 g IV; 3 g if 120 kg or more, within 60 minutes before incision.')];
    const result = await (await service()).ingest({ sourceKey: 'openfda', term: 'cefazolin', limit: 5 });
    expect(result).toMatchObject({ updated: 1, created: 0 });

    const { Document } = await entities();
    const versions = await orm.em.fork().find(
      Document,
      { externalId: 'set-cefazolin' },
      { orderBy: { version: 'asc' } }
    );
    expect(versions.map((v) => [v.version, v.status])).toEqual([
      [1, 'superseded'],
      [2, 'current']
    ]);
    expect(versions[0].supersededAt).toBeInstanceOf(Date);
  });

  it('keeps only a short excerpt of publisher-copyrighted abstracts', async () => {
    pubmed.documents = [
      {
        sourceKey: 'pubmed',
        externalId: '42785763',
        title: 'Spilled gallstones after laparoscopic cholecystectomy: a case report',
        url: 'https://pubmed.ncbi.nlm.nih.gov/42785763/',
        license: 'publisher-copyright-abstract',
        isCaseReport: true,
        meshDescriptorUis: ['D017081'],
        sections: [
          { path: 'Abstract — Background', text: 'Spilled gallstones are a rare cause of late abscess formation after laparoscopic cholecystectomy and are easily overlooked.' },
          { path: 'Abstract — Case', text: 'A long second section that must not be stored at all.' }
        ]
      }
    ];
    await (await service()).ingest({ sourceKey: 'pubmed', term: 'x', limit: 5 });

    const { Document, DocumentChunk } = await entities();
    const em = orm.em.fork();
    const doc = await em.findOneOrFail(Document, { externalId: '42785763' });
    expect(doc).toMatchObject({ licenseScope: 'excerpt_only', isCaseReport: true, meshDescriptorUis: ['D017081'] });
    const chunks = await em.find(DocumentChunk, { document: doc.id });
    const stored = chunks.map((c) => c.text).join(' ');
    expect(stored.length).toBeLessThanOrEqual(81);
    expect(stored.endsWith('…')).toBe(true);
    expect(stored).not.toContain('must not be stored');
  });

  it('stores no text for non-commercial licenses', async () => {
    pmc.documents = [
      {
        sourceKey: 'pmc_oa',
        externalId: 'PMC90000002',
        title: 'Spilled gallstones causing a late abdominal wall abscess: a case report',
        url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC90000002/',
        license: 'https://creativecommons.org/licenses/by-nc-nd/4.0/',
        isCaseReport: true,
        sections: [{ path: 'Case presentation', text: 'Full text that must not be stored.' }]
      }
    ];
    const result = await (await service()).ingest({ sourceKey: 'pmc_oa', term: 'x', limit: 5 });
    expect(result).toMatchObject({ created: 1, metadataOnly: 1, passages: 0 });

    const { Document, DocumentChunk } = await entities();
    const em = orm.em.fork();
    const doc = await em.findOneOrFail(Document, { externalId: 'PMC90000002' });
    expect(doc.licenseScope).toBe('metadata_only');
    expect(await em.count(DocumentChunk, { document: doc.id })).toBe(0);
  });

  it('withdraws a document from search when it is retracted', async () => {
    const paper: FetchedDocumentDto = {
      sourceKey: 'pmc_oa',
      externalId: 'PMC90000003',
      title: 'Outcomes of a surgical technique',
      url: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC90000003/',
      license: 'CC BY 4.0',
      sections: [{ path: 'Results', text: 'Median blood loss was 20 mL.' }]
    };
    pmc.documents = [paper];
    await (await service()).ingest({ sourceKey: 'pmc_oa', term: 'x', limit: 5 });

    const { Document, DocumentChunk } = await entities();
    const before = await orm.em.fork().findOneOrFail(Document, { externalId: 'PMC90000003' });
    expect(await orm.em.fork().count(DocumentChunk, { document: before.id })).toBe(1);

    pmc.documents = [{ ...paper, retracted: true }];
    const result = await (await service()).ingest({ sourceKey: 'pmc_oa', term: 'x', limit: 5 });
    expect(result.retracted).toBe(1);

    const after = await orm.em.fork().findOneOrFail(Document, { externalId: 'PMC90000003' });
    expect(after.status).toBe('retracted');
    expect(await orm.em.fork().count(DocumentChunk, { document: after.id })).toBe(0);
  });

  it('rejects an unknown source', async () => {
    await expect(
      (await service()).ingest({ sourceKey: 'wikipedia', term: 'x', limit: 1 })
    ).rejects.toThrow("Unknown or non-fetchable content source 'wikipedia'");
  });

  it('loads and updates MeSH descriptors', async () => {
    const xml = `<DescriptorRecordSet><DescriptorRecord><DescriptorUI>D017081</DescriptorUI><DescriptorName><String>Cholecystectomy, Laparoscopic</String></DescriptorName><TreeNumberList><TreeNumber>E04.210.240.130.500</TreeNumber></TreeNumberList><ConceptList><Concept><TermList><Term><String>Laparoscopic Cholecystectomy</String></Term></TermList></Concept></ConceptList></DescriptorRecord></DescriptorRecordSet>`;
    const ingestion = await service();
    expect(await ingestion.loadMeshConcepts(MeshDescriptorParser.parseAll(xml))).toBe(1);
    expect(
      await (await service()).loadMeshConcepts([
        { descriptorUi: 'D017081', preferredTerm: 'Cholecystectomy, Laparoscopic', synonyms: ['Laparoscopic Cholecystectomy', 'Celioscopic Cholecystectomy'], treeNumbers: ['E04.210.240.130.500'] }
      ])
    ).toBe(1);

    const { MedicalConcept } = await entities();
    const concepts = await orm.em.fork().find(MedicalConcept, {});
    expect(concepts).toHaveLength(1);
    expect(concepts[0].synonyms).toEqual(['Laparoscopic Cholecystectomy', 'Celioscopic Cholecystectomy']);
  });
});
