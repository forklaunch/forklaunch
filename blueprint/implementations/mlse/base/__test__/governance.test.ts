import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import { AnswerResponseDto } from '@forklaunch/interfaces-mlse/types';
import { LicensedContentAdapter } from '../services/licensedContentAdapter.service';
import { licenseScopeFor } from '../services/licenseGate.service';
import { SourceFetcherRegistry } from '../services/sourceFetcherRegistry.service';
import { isValidCareArea, removeIdentifiers, spokenSummary } from '../services/voice.service';

describe('removeIdentifiers', () => {
  it('removes names, record numbers, dates and contact details', () => {
    const { text, removed } = removeIdentifiers(
      'Mrs Anne Jones in bed 12, MRN 4471923, born 12/03/1961, call 555 123 4567 or anne@example.org. Her name is Anne. How much blood loss is expected?'
    );
    expect(text).not.toMatch(/Anne|Jones|4471923|1961|555|example\.org|bed 12/);
    expect(text).toContain('How much blood loss is expected?');
    expect(removed).toBeGreaterThanOrEqual(6);
  });

  it('keeps clinical numbers the classifier needs', () => {
    expect(removeIdentifiers('patient weighs 80 kg, how much propofol').text).toBe(
      'patient weighs 80 kg, how much propofol'
    );
    expect(removeIdentifiers('expected blood loss in laparoscopic cholecystectomy').removed).toBe(0);
  });

  it('runs in linear time on hostile input', () => {
    const start = performance.now();
    removeIdentifiers('1'.repeat(1500) + 'a' + 'Mr '.repeat(150));
    expect(performance.now() - start).toBeLessThan(500);
  });
});

describe('spokenSummary', () => {
  const answer = (overrides: Partial<AnswerResponseDto>): AnswerResponseDto => ({
    answerId: 'a',
    query: 'q',
    queryClass: 'literature_lookup',
    kind: 'answer',
    notice: '',
    sections: [],
    sources: [],
    ...overrides
  });

  it('reads fixed messages as written', () => {
    expect(spokenSummary(answer({ kind: 'emergency', message: 'Follow your emergency protocol.' }))).toBe(
      'Follow your emergency protocol.'
    );
  });

  it('reads verified sentences without citations and points to the screen', () => {
    const spoken = spokenSummary(
      answer({
        sections: [
          { key: 'blood', label: 'Blood', status: 'answered', removed: 0, sentences: [{ text: 'Median blood loss was 20 mL.', citations: ['c1'] }] },
          { key: 'positioning', label: 'Positioning', status: 'insufficient_evidence', removed: 0, sentences: [] }
        ],
        sources: [{} as AnswerResponseDto['sources'][number], {} as AnswerResponseDto['sources'][number]]
      })
    );
    expect(spoken).toBe('Median blood loss was 20 mL. All 2 sources are on screen.');
  });

  it('never reads out label text', () => {
    expect(spokenSummary(answer({ kind: 'label_range', sources: [{} as AnswerResponseDto['sources'][number]] }))).toBe(
      'The label dosing section is on screen, quoted as written. The source is on screen.'
    );
  });
});

describe('care areas', () => {
  it('accepts lowercase area names only', () => {
    expect(isValidCareArea('operating_room')).toBe(true);
    expect(isValidCareArea('Emergency')).toBe(false);
    expect(isValidCareArea("or'; drop table")).toBe(false);
  });
});

describe('licensed content', () => {
  const inner: SourceFetcher = {
    sourceKey: 'guideline_feed',
    fetchDocuments: async () => [
      { sourceKey: 'guideline_feed', externalId: 'G1', title: 'Guideline', url: 'https://example.org/g1', license: 'All rights reserved', sections: [] }
    ]
  };

  it('stores documents under the contract terms, not the feed string', async () => {
    const full = await new LicensedContentAdapter(inner, { scope: 'full_text' }).fetchDocuments({ term: 'x', limit: 1 });
    const excerpt = await new LicensedContentAdapter(inner, { scope: 'excerpt_only' }).fetchDocuments({ term: 'x', limit: 1 });
    expect(licenseScopeFor(full[0].license)).toBe('full_text');
    expect(licenseScopeFor(excerpt[0].license)).toBe('excerpt_only');
    // the feed's own string would have allowed nothing
    expect(licenseScopeFor('All rights reserved')).toBe('metadata_only');
  });

  it('marks licensed fetchers in the registry', () => {
    const registry = new SourceFetcherRegistry([
      new LicensedContentAdapter(inner, { scope: 'full_text' }),
      { sourceKey: 'pubmed', fetchDocuments: async () => [] }
    ]);
    expect(registry.isLicensed('guideline_feed')).toBe(true);
    expect(registry.isLicensed('pubmed')).toBe(false);
  });
});
