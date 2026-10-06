import { FetchedDocumentDto } from '@forklaunch/interfaces-mlse/types';
import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  retrievalCompleteness,
  summarizeEvaluation
} from '../services/evaluation.service';
import { applyLicense } from '../services/licenseText.service';
import {
  LiveResultCache,
  LiveRetrievalService
} from '../services/liveRetrieval.service';
import {
  LexicalReranker,
  queryTerms,
  reciprocalRankFusion
} from '../services/ranking.service';
import { SourceFetcherRegistry } from '../services/sourceFetcherRegistry.service';

describe('reciprocalRankFusion', () => {
  it('rewards items that several retrievers agree on', () => {
    const fused = reciprocalRankFusion([
      ['a', 'b', 'c'],
      ['c', 'a'],
      ['d']
    ]);
    expect(fused.map((f) => f.id)).toEqual(['a', 'c', 'd', 'b']);
    expect(fused.find((f) => f.id === 'a')?.lists).toEqual([0, 1]);
  });

  it('counts an id once per list even if repeated', () => {
    const [only] = reciprocalRankFusion([['a', 'a', 'a']]);
    expect(only.score).toBeCloseTo(1 / 61, 10);
  });
});

describe('LexicalReranker', () => {
  const reranker = new LexicalReranker();

  it('scores passages by how much of the query they cover', async () => {
    const scores = await reranker.rerank('cefazolin surgical prophylaxis', [
      { id: 'full', text: 'Cefazolin for surgical prophylaxis before incision.' },
      { id: 'partial', text: 'Cefazolin is a cephalosporin.' },
      { id: 'none', text: 'Gallbladder volvulus is rare.' }
    ]);
    const byId = Object.fromEntries(scores.map((s) => [s.id, s.score]));
    expect(byId.full).toBeGreaterThan(byId.partial);
    expect(byId.partial).toBeGreaterThan(byId.none);
    expect(byId.none).toBe(0);
  });

  it('normalizes plurals and ignores stopwords', () => {
    expect(queryTerms('What are the complications of the incisions?')).toEqual([
      'complication',
      'incision'
    ]);
  });
});

class StubFetcher implements SourceFetcher {
  calls = 0;
  constructor(
    readonly sourceKey: string,
    private readonly behaviour: () => Promise<FetchedDocumentDto[]>
  ) {}
  async fetchDocuments() {
    this.calls++;
    return this.behaviour();
  }
}

const doc = (overrides: Partial<FetchedDocumentDto>): FetchedDocumentDto => ({
  sourceKey: 'pubmed',
  externalId: '1',
  title: 'A title',
  url: 'https://example.org/1',
  license: 'CC BY 4.0',
  sections: [{ path: 'Abstract', text: 'Median blood loss was 20 mL.' }],
  ...overrides
});

describe('LiveRetrievalService', () => {
  it('queries live sources in parallel and reports each outcome', async () => {
    const ok = new StubFetcher('pubmed', async () => [doc({})]);
    const slow = new StubFetcher('openfda', () => new Promise((r) => setTimeout(() => r([]), 500)));
    const broken = new StubFetcher('clinicaltrials', async () => {
      throw new Error('HTTP 503');
    });
    const service = new LiveRetrievalService(
      new SourceFetcherRegistry([ok, slow, broken]),
      ['pubmed', 'openfda', 'clinicaltrials'],
      undefined,
      { timeoutMs: 50 }
    );

    const result = await service.retrieve('blood loss');
    expect(result.sources).toEqual([
      { sourceKey: 'pubmed', status: 'ok', documents: 1 },
      { sourceKey: 'openfda', status: 'timeout', documents: 0, error: undefined },
      { sourceKey: 'clinicaltrials', status: 'error', documents: 0, error: 'HTTP 503' }
    ]);
    expect(result.passages).toHaveLength(1);
    expect(result.passages[0]).toMatchObject({
      passageId: 'live:pubmed:1:0',
      origin: 'live',
      licenseScope: 'full_text'
    });
  });

  it('applies the same license rules as ingestion and drops retractions', async () => {
    const fetcher = new StubFetcher('pubmed', async () => [
      doc({ externalId: 'nc', license: 'CC BY-NC 4.0' }),
      doc({ externalId: 'abstract', license: 'publisher-copyright-abstract', sections: [{ path: 'Abstract', text: 'word '.repeat(300).trim() }] }),
      doc({ externalId: 'retracted', retracted: true })
    ]);
    const service = new LiveRetrievalService(new SourceFetcherRegistry([fetcher]), ['pubmed'], undefined, { excerptChars: 100 });
    const { passages } = await service.retrieve('x');
    expect(passages.map((p) => p.externalId)).toEqual(['abstract']);
    expect(passages[0].text.length).toBeLessThanOrEqual(101);
    expect(passages[0].licenseScope).toBe('excerpt_only');
  });

  it('serves repeated terms from the cache without calling the source', async () => {
    const store = new Map<string, unknown>();
    const cache: LiveResultCache = {
      peekRecord: async (key) => store.has(key),
      readRecord: async <T>(key: string) => ({ value: store.get(key) as T }),
      putRecord: async ({ key, value }) => {
        store.set(key, value);
      }
    };
    const fetcher = new StubFetcher('pubmed', async () => [doc({})]);
    const service = new LiveRetrievalService(new SourceFetcherRegistry([fetcher]), ['pubmed'], cache);

    await service.retrieve('Blood  Loss');
    const second = await service.retrieve('blood loss');
    expect(fetcher.calls).toBe(1);
    expect(second.sources[0].status).toBe('cached');
    expect(second.passages).toHaveLength(1);
  });

  it('keeps working when the cache fails', async () => {
    const cache: LiveResultCache = {
      peekRecord: async () => {
        throw new Error('redis down');
      },
      readRecord: async () => {
        throw new Error('redis down');
      },
      putRecord: async () => {
        throw new Error('redis down');
      }
    };
    const fetcher = new StubFetcher('pubmed', async () => [doc({})]);
    const service = new LiveRetrievalService(new SourceFetcherRegistry([fetcher]), ['pubmed'], cache);
    const result = await service.retrieve('x');
    expect(result.sources[0].status).toBe('ok');
  });

  it('only queries the requested sources', async () => {
    const pubmed = new StubFetcher('pubmed', async () => []);
    const openfda = new StubFetcher('openfda', async () => []);
    const service = new LiveRetrievalService(new SourceFetcherRegistry([pubmed, openfda]), ['pubmed', 'openfda']);
    await service.retrieve('x', ['openfda']);
    expect([pubmed.calls, openfda.calls]).toEqual([0, 1]);
  });
});

describe('applyLicense', () => {
  const sections = [
    { path: 'A', text: 'First section text.' },
    { path: 'B', text: 'Second section.' }
  ];
  it('keeps everything, an excerpt, or nothing', () => {
    expect(applyLicense(sections, 'full_text')).toEqual(sections);
    expect(applyLicense(sections, 'metadata_only')).toEqual([]);
    expect(applyLicense(sections, 'excerpt_only', 10)).toEqual([{ path: 'A', text: 'First…' }]);
  });
});

describe('retrievalCompleteness', () => {
  const question = {
    id: 'q1',
    query: 'x',
    expectedSources: [
      { sourceKey: 'openfda', externalId: 'a' },
      { sourceKey: 'pmc_oa', externalId: 'b' },
      { sourceKey: 'pmc_oa', externalId: 'b' }
    ]
  };

  it('counts distinct expected documents found in the top k', () => {
    const score = retrievalCompleteness(
      question,
      [
        { sourceKey: 'openfda', externalId: 'a' },
        { sourceKey: 'pubmed', externalId: 'z' },
        { sourceKey: 'pmc_oa', externalId: 'b' }
      ],
      2
    );
    expect(score).toMatchObject({ found: 1, expected: 2, completeness: 0.5 });
    expect(score.missing).toEqual([{ sourceKey: 'pmc_oa', externalId: 'b' }]);
  });

  it('summarizes a run', () => {
    const summary = summarizeEvaluation([
      { id: 'a', query: '', found: 1, expected: 1, completeness: 1, missing: [] },
      { id: 'b', query: '', found: 0, expected: 2, completeness: 0, missing: [] }
    ]);
    expect(summary).toEqual({ questions: 2, meanCompleteness: 0.5, fullyAnswered: 1 });
  });
});
