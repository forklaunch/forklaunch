import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { FetchLike } from '../domain/http';
import { MedlinePlusFetcher, summarySections } from '../services/fetchers/medlinePlusFetcher.service';
import { licenseScopeFor } from '../services/licenseGate.service';

const sample = readFileSync(path.join(__dirname, 'fixtures', 'medlineplus-heart-attack.xml'), 'utf-8');

describe('MedlinePlusFetcher', () => {
  it('turns health topics into question-headed, public-domain documents', async () => {
    const requests: string[] = [];
    const fetchImpl: FetchLike = async (url) => {
      requests.push(url);
      return { ok: true, status: 200, text: async () => sample };
    };
    const [heartAttack] = await new MedlinePlusFetcher(fetchImpl).fetchDocuments({ term: 'heart attack', limit: 2 });

    expect(requests[0]).toContain('db=healthTopics&term=heart%20attack');
    expect(heartAttack).toMatchObject({
      sourceKey: 'medlineplus',
      externalId: 'heartattack',
      title: 'Heart Attack (MedlinePlus, National Library of Medicine)',
      url: 'https://medlineplus.gov/heartattack.html'
    });
    expect(licenseScopeFor(heartAttack.license)).toBe('full_text');

    const symptoms = heartAttack.sections.find((s) => s.path === 'What are the symptoms of a heart attack?');
    expect(symptoms?.text).toContain('Chest discomfort.');
    expect(symptoms?.text).toContain('Shortness of breath.');
    // search-term highlighting markup is gone
    expect(heartAttack.sections.every((s) => !s.text.includes('<span'))).toBe(true);
    expect(heartAttack.sections[0].path).toBe('What is a heart attack?');
  });

  it('splits a summary on its question headings', () => {
    expect(summarySections('What is it?<p>A condition.</p>Who gets it?<ul><li>Adults</li><li>Children</li></ul>')).toEqual([
      { path: 'What is it?', text: 'A condition.' },
      { path: 'Who gets it?', text: 'Adults Children' }
    ]);
  });
});
