import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { FetchLike } from '../domain/http';
import { OpenFdaFetcher } from '../services/fetchers/openFdaFetcher.service';
import {
  isDosingSection,
  labelIsForQueryDrug,
  labelNames,
  labelsForQueryDrug
} from '../services/labelMatch.service';
import { classifyQuery } from '../services/queryClassifier.service';

// A recorded openFDA response holding the distractor labels live openFDA
// returned in review, run through the real fetcher so the titles have the
// exact shape the answer service sees.
const recorded = readFileSync(
  path.join(__dirname, 'fixtures', 'openfda-dose-distractors.json'),
  'utf-8'
);

async function labels() {
  const fetchImpl: FetchLike = async () => ({
    ok: true,
    status: 200,
    text: async () => recorded
  });
  return new OpenFdaFetcher(fetchImpl).fetchDocuments({ term: 'x', limit: 20 });
}

async function dosingLabelsFor(query: string): Promise<string[]> {
  const dosing = (await labels()).filter((label) =>
    label.sections.some((s) => isDosingSection(s.path))
  );
  return labelsForQueryDrug(
    dosing,
    query,
    classifyQuery(query).subjectTerms
  ).map((label) => label.externalId);
}

describe('labelNames', () => {
  it('reads the brand and the generic name, with and without the salt', () => {
    expect(labelNames('Morphine Sulfate — Example Pharma')).toEqual([
      'morphine sulfate',
      'morphine'
    ]);
    expect(labelNames('Lantus (insulin glargine) — Example Pharma')).toEqual([
      'lantus',
      'insulin glargine'
    ]);
    expect(labelNames('Cefazolin for injection, USP — Example Pharma')).toEqual(
      ['cefazolin for injection usp', 'cefazolin']
    );
  });
});

describe('labelIsForQueryDrug, against distractor labels', () => {
  it('quotes nothing for "blood sugar": no label is for sugar', async () => {
    expect(
      await dosingLabelsFor(
        'what dose of insulin should I take for a blood sugar of 300'
      )
    ).toEqual([]);
  });

  it('quotes morphine for a morphine question, not an acetaminophen "pain reliever"', async () => {
    expect(
      await dosingLabelsFor(
        'my patient is in severe pain, how much morphine should I give'
      )
    ).toEqual(['synthetic-morphine']);
  });

  it('matches a drug by its generic name without the salt, or by its brand', async () => {
    expect(
      await dosingLabelsFor('cefazolin dose surgical prophylaxis')
    ).toEqual(['synthetic-cefazolin']);
    expect(await dosingLabelsFor('lantus dose')).toEqual([
      'synthetic-insulin-glargine'
    ]);
    expect(await dosingLabelsFor('insulin glargine dose')).toEqual([
      'synthetic-insulin-glargine'
    ]);
  });

  it('never matches on one shared word of a longer name', async () => {
    expect(await dosingLabelsFor('extra strength dose for pain')).toEqual([]);
    expect(await dosingLabelsFor('sugar free cough syrup dose')).toEqual([]);
  });
});

describe('labelsForQueryDrug, against store brands named after the indication', () => {
  it('never quotes "Pain Relief" (acetaminophen) for an ibuprofen question', async () => {
    expect(
      await dosingLabelsFor('what is the dose of ibuprofen for pain relief')
    ).toEqual(['synthetic-ibuprofen']);
    expect(
      labelIsForQueryDrug(
        'Pain Relief (acetaminophen) — Acme',
        'what is the dose of ibuprofen for pain relief'
      )
    ).toBe(false);
  });

  it('titles an openFDA label that has a brand but no generic name so it never matches', async () => {
    const body = JSON.stringify({
      results: [
        {
          set_id: 'synthetic-brand-only',
          effective_time: '20250101',
          dosage_and_administration: ['Adults: 2 tablets.'],
          openfda: { brand_name: ['Pain Relief'], manufacturer_name: ['Acme'] }
        }
      ]
    });
    const fetchImpl: FetchLike = async () => ({ ok: true, status: 200, text: async () => body });
    const [label] = await new OpenFdaFetcher(fetchImpl).fetchDocuments({ term: 'x', limit: 1 });
    expect(label.title).toBe('Pain Relief, generic name not listed — Acme');
    expect(labelsForQueryDrug([label], 'what is the dose of ibuprofen for pain relief', ['ibuprofen', 'pain', 'relief'])).toEqual([]);
    expect(labelsForQueryDrug([label], 'pain relief dose', ['pain', 'relief'])).toEqual([]);
  });

  it('never reads a brand with no generic name as a drug name', () => {
    expect(labelIsForQueryDrug('Pain Relief, generic name not listed — Acme', 'pain relief dose')).toBe(false);
    expect(labelsForQueryDrug([{ title: 'Pain Relief, generic name not listed — Acme' }], 'pain relief dose', ['pain', 'relief'])).toEqual([]);
  });

  it('quotes nothing when the named drug has no label and only the brand matches', async () => {
    expect(
      await dosingLabelsFor('what is the dose of naproxen for pain relief')
    ).toEqual([]);
  });

  it('quotes nothing for "Sleep Aid": the brand is doxylamine on one label, diphenhydramine on another', async () => {
    expect(await dosingLabelsFor('sleep aid dose for adults')).toEqual([]);
    expect(
      await dosingLabelsFor('melatonin or a sleep aid, what dose')
    ).toEqual([]);
  });

  it('quotes nothing for "Allergy Relief": the brand is cetirizine on one label, loratadine on another', async () => {
    expect(await dosingLabelsFor('allergy relief dose')).toEqual([]);
  });

  it('quotes by generic name even when a store brand shares a word of the query', async () => {
    expect(await dosingLabelsFor('loratadine dose for allergy relief')).toEqual(
      ['synthetic-allergy-relief-loratadine']
    );
    expect(await dosingLabelsFor('doxylamine dose as a sleep aid')).toEqual([
      'synthetic-sleep-aid-doxylamine'
    ]);
  });

  it('quotes a brand whose name is the only drug the query names, and only one drug carries it', async () => {
    expect(await dosingLabelsFor('pain relief dose')).toEqual([
      'synthetic-pain-relief-acetaminophen'
    ]);
  });
});

describe('isDosingSection', () => {
  it('is the dosing section only, never Overdosage', () => {
    expect(isDosingSection('Dosage and Administration')).toBe(true);
    expect(isDosingSection('Overdosage')).toBe(false);
    expect(isDosingSection('Warnings')).toBe(false);
  });
});
