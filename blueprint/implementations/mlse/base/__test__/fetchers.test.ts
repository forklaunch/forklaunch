import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { FetchLike, SourceRequestError } from '../domain/http';
import { ClinicalTrialsFetcher } from '../services/fetchers/clinicalTrialsFetcher.service';
import {
  DailyMedFetcher,
  dailyMedDateToIso
} from '../services/fetchers/dailyMedFetcher.service';
import { OpenFdaFetcher } from '../services/fetchers/openFdaFetcher.service';
import { PmcOaFetcher } from '../services/fetchers/pmcOaFetcher.service';
import { PubMedFetcher } from '../services/fetchers/pubmedFetcher.service';
import { licenseScopeFor } from '../services/licenseGate.service';

const fixture = (name: string) =>
  readFileSync(path.join(__dirname, 'fixtures', name), 'utf-8');

// Serves recorded responses by URL substring and records every request.
function recordedFetch(routes: [match: string, body: string, status?: number][]) {
  const requests: string[] = [];
  const fetchImpl: FetchLike = async (url) => {
    requests.push(url);
    const route = routes.find(([match]) => url.includes(match));
    const status = route ? (route[2] ?? 200) : 404;
    return {
      ok: status >= 200 && status < 300,
      status,
      text: async () => (route ? route[1] : '')
    };
  };
  return { fetchImpl, requests };
}

const ncbi = { tool: 'mlse-test', email: 'test@example.com' };

describe('OpenFdaFetcher', () => {
  it('turns a label into a sectioned, public-domain document', async () => {
    const { fetchImpl, requests } = recordedFetch([
      ['api.fda.gov', fixture('openfda-labels.json')]
    ]);
    const [doc] = await new OpenFdaFetcher(fetchImpl).fetchDocuments({
      term: 'cefazolin',
      limit: 1
    });

    expect(decodeURIComponent(requests[0])).toContain(
      'search=openfda.generic_name:"cefazolin"+openfda.brand_name:"cefazolin"'
    );
    expect(doc.externalId).toBe('00dd6da7-50a8-44c9-b2dc-7948973df392');
    expect(doc.title).toContain('Cefazolin');
    expect(doc.publishedAt).toBe('2026-01-07');
    expect(licenseScopeFor(doc.license)).toBe('full_text');
    expect(doc.sections.map((s) => s.path)).toEqual(
      expect.arrayContaining(['Indications and Usage', 'Dosage and Administration', 'Contraindications'])
    );
    expect(doc.sections.every((s) => s.text.length > 0)).toBe(true);
  });

  it('treats openFDA "no matches" (HTTP 404) as an empty result', async () => {
    const { fetchImpl } = recordedFetch([]);
    await expect(
      new OpenFdaFetcher(fetchImpl).fetchDocuments({ term: 'nonexistent', limit: 5 })
    ).resolves.toEqual([]);
  });

  it('never puts the API key in an error message', async () => {
    const { fetchImpl } = recordedFetch([['api.fda.gov', '', 500]]);
    const fetcher = new OpenFdaFetcher(fetchImpl, { apiKey: 'secret-key' });
    const error = await fetcher.fetchDocuments({ term: 'x', limit: 1 }).catch((e) => e);
    expect(error).toBeInstanceOf(SourceRequestError);
    expect(error.message).not.toContain('secret-key');
    expect(error.message).toContain('api_key=***');
  });
});

describe('ClinicalTrialsFetcher', () => {
  it('maps a study to a document with status and eligibility', async () => {
    const { fetchImpl, requests } = recordedFetch([
      ['clinicaltrials.gov', fixture('ctgov-studies.json')]
    ]);
    const [doc] = await new ClinicalTrialsFetcher(fetchImpl).fetchDocuments({
      term: 'laparoscopic cholecystectomy',
      limit: 1
    });

    expect(requests[0]).toContain('query.term=laparoscopic%20cholecystectomy');
    expect(doc.externalId).toBe('NCT06177873');
    expect(doc.url).toBe('https://clinicaltrials.gov/study/NCT06177873');
    expect(doc.sections.find((s) => s.path === 'Status')?.text).toContain('Overall status: completed.');
    expect(doc.sections.find((s) => s.path === 'Conditions')?.text).toBe('Laparoscopic Cholecystectomy');
    expect(doc.sections.some((s) => s.path === 'Eligibility')).toBe(true);
    expect(licenseScopeFor(doc.license)).toBe('full_text');
  });
});

describe('PubMedFetcher', () => {
  const run = async () => {
    const { fetchImpl, requests } = recordedFetch([
      ['esearch.fcgi', JSON.stringify({ esearchresult: { idlist: ['42785763', '42784035', '10000001'] } })],
      ['efetch.fcgi', fixture('pubmed-efetch.xml')]
    ]);
    const docs = await new PubMedFetcher(fetchImpl, ncbi).fetchDocuments({
      term: 'laparoscopic cholecystectomy AND case reports[pt]',
      limit: 3
    });
    return { docs, requests };
  };

  it('identifies itself to NCBI on every request', async () => {
    const { requests } = await run();
    expect(requests).toHaveLength(2);
    for (const url of requests) {
      expect(url).toContain('tool=mlse-test');
      expect(url).toContain('email=test%40example.com');
    }
  });

  it('parses citations, case-report flags and MeSH descriptors', async () => {
    const { docs } = await run();
    const [first] = docs;
    expect(docs.map((d) => d.externalId)).toEqual(['42785763', '42784035', '10000001']);
    expect(first.url).toBe('https://pubmed.ncbi.nlm.nih.gov/42785763/');
    expect(first.isCaseReport).toBe(true);
    expect(first.title.length).toBeGreaterThan(10);
    expect(first.sections[0].path.startsWith('Abstract')).toBe(true);
    expect(docs.flatMap((d) => d.meshDescriptorUis ?? [])).toContain('D017081');
  });

  it('marks abstracts as excerpt-only, not full text', async () => {
    const { docs } = await run();
    expect(licenseScopeFor(docs[0].license)).toBe('excerpt_only');
  });

  it('flags retracted publications and flattens inline markup', async () => {
    const { docs } = await run();
    const retracted = docs.find((d) => d.externalId === '10000001');
    expect(retracted?.retracted).toBe(true);
    expect(retracted?.title).toBe('A retracted study of gallbladder surgery outcomes.');
    expect(retracted?.sections[0]).toEqual({
      path: 'Abstract — Results',
      text: 'Outcomes were reported & later withdrawn.'
    });
    expect(retracted?.publishedAt).toBe('2021-03');
  });

  it('skips efetch when the search finds nothing', async () => {
    const { fetchImpl, requests } = recordedFetch([
      ['esearch.fcgi', JSON.stringify({ esearchresult: { idlist: [] } })]
    ]);
    await expect(
      new PubMedFetcher(fetchImpl, ncbi).fetchDocuments({ term: 'x', limit: 5 })
    ).resolves.toEqual([]);
    expect(requests).toHaveLength(1);
  });
});

describe('PmcOaFetcher', () => {
  const run = async () => {
    const { fetchImpl, requests } = recordedFetch([
      ['esearch.fcgi', JSON.stringify({ esearchresult: { idlist: ['90000001', '90000002'] } })],
      ['efetch.fcgi', fixture('pmc-efetch.xml')]
    ]);
    const docs = await new PmcOaFetcher(fetchImpl, ncbi).fetchDocuments({
      term: 'cholecystectomy',
      limit: 2
    });
    return { docs, requests };
  };

  it('restricts the search to the open-access subset', async () => {
    const { requests } = await run();
    expect(decodeURIComponent(requests[0])).toContain('(cholecystectomy) AND open access[filter]');
  });

  it('keeps nested and structured sections with their headings', async () => {
    const { docs } = await run();
    const article = docs[0];
    expect(article.externalId).toBe('PMC90000001');
    expect(article.title).toBe(
      'Outcomes of laparoscopic cholecystectomy with common bile duct exploration'
    );
    expect(article.publishedAt).toBe('2026-07-03');
    expect(article.sections.map((s) => s.path)).toEqual([
      'Abstract › Background',
      'Abstract › Results',
      'Introduction',
      'Methods › Surgical technique'
    ]);
    expect(article.sections[1].text).toBe('Median blood loss was 20 mL (IQR 10–50).');
    expect(article.sections[2].text).toBe(
      'Gallstones are among the most common biliary tract diseases worldwide [1].'
    );
  });

  it('passes each article license through to the license gate', async () => {
    const { docs } = await run();
    const [openArticle, ncCaseReport] = docs;
    expect(licenseScopeFor(openArticle.license)).toBe('full_text');
    expect(ncCaseReport.isCaseReport).toBe(true);
    expect(ncCaseReport.license).toBe('https://creativecommons.org/licenses/by-nc-nd/4.0/');
    expect(licenseScopeFor(ncCaseReport.license)).toBe('metadata_only');
  });
});

describe('DailyMedFetcher', () => {
  it('lists current label versions without body text', async () => {
    const { fetchImpl } = recordedFetch([['dailymed.nlm.nih.gov', fixture('dailymed-spls.json')]]);
    const docs = await new DailyMedFetcher(fetchImpl).fetchDocuments({ term: 'cefazolin', limit: 2 });
    expect(docs).toHaveLength(2);
    expect(docs[0].externalId).toBe('1999084a-124c-45f9-801f-416a1b942c96');
    expect(docs[0].publishedAt).toBe('2026-09-18');
    expect(docs[0].sections).toEqual([]);
    expect(licenseScopeFor(docs[0].license)).toBe('full_text');
  });

  it('parses DailyMed dates', () => {
    expect(dailyMedDateToIso('Sep 18, 2026')).toBe('2026-09-18');
    expect(dailyMedDateToIso('January 5, 2025')).toBe('2025-01-05');
    expect(dailyMedDateToIso('not a date')).toBeUndefined();
  });
});
