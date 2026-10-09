import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { FetchLike } from '../domain/http';
import { OpenFdaFetcher } from '../services/fetchers/openFdaFetcher.service';
import { isDosingSection, labelIsForQueryDrug, labelNames } from '../services/labelMatch.service';

// A recorded openFDA response holding the distractor labels live openFDA
// returned in review, run through the real fetcher so the titles have the
// exact shape the answer service sees.
const recorded = readFileSync(path.join(__dirname, 'fixtures', 'openfda-dose-distractors.json'), 'utf-8');

async function labels() {
  const fetchImpl: FetchLike = async () => ({ ok: true, status: 200, text: async () => recorded });
  return new OpenFdaFetcher(fetchImpl).fetchDocuments({ term: 'x', limit: 10 });
}

async function dosingLabelsFor(query: string): Promise<string[]> {
  return (await labels())
    .filter((label) => label.sections.some((s) => isDosingSection(s.path)))
    .filter((label) => labelIsForQueryDrug(label.title, query))
    .map((label) => label.externalId);
}

describe('labelNames', () => {
  it('reads the brand and the generic name, with and without the salt', () => {
    expect(labelNames('Morphine Sulfate — Example Pharma')).toEqual(['morphine sulfate', 'morphine']);
    expect(labelNames('Lantus (insulin glargine) — Example Pharma')).toEqual(['lantus', 'insulin glargine']);
    expect(labelNames('Cefazolin for injection, USP — Example Pharma')).toEqual([
      'cefazolin for injection usp',
      'cefazolin'
    ]);
  });
});

describe('labelIsForQueryDrug, against distractor labels', () => {
  it('quotes nothing for "blood sugar": no label is for sugar', async () => {
    expect(await dosingLabelsFor('what dose of insulin should I take for a blood sugar of 300')).toEqual([]);
  });

  it('quotes morphine for a morphine question, not an acetaminophen "pain reliever"', async () => {
    expect(
      await dosingLabelsFor('my patient is in severe pain, how much morphine should I give')
    ).toEqual(['synthetic-morphine']);
  });

  it('matches a drug by its generic name without the salt, or by its brand', async () => {
    expect(await dosingLabelsFor('cefazolin dose surgical prophylaxis')).toEqual(['synthetic-cefazolin']);
    expect(await dosingLabelsFor('lantus dose')).toEqual(['synthetic-insulin-glargine']);
    expect(await dosingLabelsFor('insulin glargine dose')).toEqual(['synthetic-insulin-glargine']);
  });

  it('never matches on one shared word of a longer name', async () => {
    expect(await dosingLabelsFor('extra strength dose for pain')).toEqual([]);
    expect(await dosingLabelsFor('sugar free cough syrup dose')).toEqual([]);
  });
});

describe('isDosingSection', () => {
  it('is the dosing section only, never Overdosage', () => {
    expect(isDosingSection('Dosage and Administration')).toBe(true);
    expect(isDosingSection('Overdosage')).toBe(false);
    expect(isDosingSection('Warnings')).toBe(false);
  });
});
