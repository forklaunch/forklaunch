import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { FetchLike, SourceRequestError } from '../domain/http';
import { ClinicalTrialsFetcher } from '../services/fetchers/clinicalTrialsFetcher.service';
import {
  DailyMedFetcher,
  dailyMedDateToIso
} from '../services/fetchers/dailyMedFetcher.service';
import { GuidelineFetcher } from '../services/fetchers/guidelineFetcher.service';
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

  it('looks up each longer word of a phrase as a possible drug name', async () => {
    const { fetchImpl, requests } = recordedFetch([['api.fda.gov', fixture('openfda-labels.json')]]);
    await new OpenFdaFetcher(fetchImpl).fetchDocuments({ term: 'cefazolin dose surgical prophylaxis', limit: 3 });
    const url = decodeURIComponent(requests[0]);
    expect(url).toContain('openfda.generic_name:"cefazolin dose surgical prophylaxis"');
    expect(url).toContain('openfda.generic_name:"cefazolin"+openfda.brand_name:"cefazolin"');
    expect(url).toContain('openfda.brand_name:"prophylaxis"');
    expect(url).not.toContain('"dos"');
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

  it('restricts the search to the open-access subset, most relevant first', async () => {
    const { requests } = await run();
    expect(decodeURIComponent(requests[0])).toContain('(cholecystectomy) AND open access[filter]');
    expect(requests[0]).toContain('sort=relevance');
  });

  // declarations, funding and AI-use statements are left out, nested or not
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

describe('GuidelineFetcher', () => {
  const record = (pmid: string, title: string, options: { pmcid?: string; abstract?: boolean } = {}) =>
    `<PubmedArticle><MedlineCitation><PMID>${pmid}</PMID><Article><Journal><JournalIssue><PubDate><Year>2025</Year></PubDate></JournalIssue></Journal>` +
    `<ArticleTitle>${title}</ArticleTitle>` +
    (options.abstract === false ? '' : `<Abstract><AbstractText>Recommendations for ${title.toLowerCase()}</AbstractText></Abstract>`) +
    `<PublicationTypeList><PublicationType>Practice Guideline</PublicationType></PublicationTypeList></Article>` +
    `<MeshHeadingList><MeshHeading><DescriptorName UI="D001064">Appendicitis</DescriptorName></MeshHeading></MeshHeadingList></MedlineCitation>` +
    `<PubmedData><ArticleIdList><ArticleId IdType="pubmed">${pmid}</ArticleId>${options.pmcid ? `<ArticleId IdType="pmc">${options.pmcid}</ArticleId>` : ''}</ArticleIdList>` +
    // a reference's PMC id is not the guideline's own
    `<ReferenceList><Reference><ArticleIdList><ArticleId IdType="pmc">PMC999</ArticleId></ArticleIdList></Reference></ReferenceList></PubmedData></PubmedArticle>`;
  const pubmed =
    '<PubmedArticleSet>' +
    record('101', 'Open appendicitis guideline', { pmcid: 'PMC1' }) +
    record('102', 'Society appendicitis guideline', { pmcid: 'PMC2' }) +
    record('103', 'Appendicitis consensus statement') +
    record('104', 'Appendicitis guideline without abstract', { abstract: false }) +
    '</PubmedArticleSet>';
  const pmc =
    '<pmc-articleset><article article-type="research-article"><front><article-meta>' +
    '<article-id pub-id-type="pmcid">PMC1</article-id><title-group><article-title>Open appendicitis guideline</article-title></title-group>' +
    '<permissions><license><ali:license_ref xmlns:ali="http://www.niso.org/schemas/ali/1.0/">https://creativecommons.org/licenses/by/4.0/</ali:license_ref></license></permissions>' +
    '</article-meta></front><body><sec><title>Recommendations</title><p>Laparoscopic appendectomy is recommended.</p></sec></body></article></pmc-articleset>';

  it('finds recent guidelines in PubMed, in full where PMC holds an openly licensed copy', async () => {
    const { fetchImpl, requests } = recordedFetch([
      ['esearch.fcgi?db=pubmed', JSON.stringify({ esearchresult: { idlist: ['101', '102', '103', '104'] } })],
      ['efetch.fcgi?db=pubmed', pubmed],
      // PMC2 is a copyrighted copy: not open access, so never downloaded
      ['esearch.fcgi?db=pmc', JSON.stringify({ esearchresult: { idlist: ['1'] } })],
      ['efetch.fcgi?db=pmc', pmc]
    ]);
    const docs = await new GuidelineFetcher(fetchImpl, ncbi, { now: () => new Date('2026-10-09') }).fetchDocuments({
      term: 'appendicitis',
      limit: 5
    });

    const search = decodeURIComponent(requests.find((url) => url.includes('esearch.fcgi?db=pubmed'))!);
    expect(search).toContain('(appendicitis) AND (guideline[pt] OR practice guideline[pt]) AND 2016:3000[dp]');
    const openAccess = decodeURIComponent(requests.find((url) => url.includes('esearch.fcgi?db=pmc'))!);
    expect(openAccess).toContain('(1[uid] OR 2[uid]) AND open access[filter]');
    expect(requests.find((url) => url.includes('efetch.fcgi?db=pmc'))).toContain('id=1&');

    expect(docs.map((d) => [d.sourceKey, d.externalId, licenseScopeFor(d.license)])).toEqual([
      ['guidelines', 'PMC1', 'full_text'],
      ['guidelines', '102', 'excerpt_only'],
      ['guidelines', '103', 'excerpt_only']
    ]);
    expect(docs[0].sections.map((s) => s.text)).toContain('Laparoscopic appendectomy is recommended.');
    expect(docs[0].meshDescriptorUis).toEqual(['D001064']);
  });
});
