import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { FetchLike } from '../domain/http';
import { ImageSearchService } from '../services/imageSearch.service';

const OPENI = readFileSync(join(__dirname, 'fixtures', 'openi-search.json'), 'utf8');

function fakeFetch(body: string, requests: string[] = [], status = 200): FetchLike {
  return async (url) => {
    requests.push(url);
    return { ok: status === 200, status, text: async () => body };
  };
}

describe('ImageSearchService', () => {
  it('returns only figures licensed for commercial reuse, with their article and license', async () => {
    const { images, status } = await new ImageSearchService(fakeFetch(OPENI)).search('appendectomy');

    expect(status).toBe('ok');
    // of 12 figures: 5 unlicensed, 3 NonCommercial or NoDerivatives
    expect(images.map((i) => i.pmcid)).toEqual(['PMC11169428', 'PMC7486639', 'PMC10625278', 'PMC12263465']);
    expect(images.every((i) => i.license === 'CC BY 4.0')).toBe(true);
    const [first] = images;
    expect(first.imageUrl).toMatch(/^https:\/\/openi\.nlm\.nih\.gov\/imgs\/512\//);
    expect(first.thumbnailUrl).toMatch(/^https:\/\/openi\.nlm\.nih\.gov\/imgs\/137\//);
    expect(first.articleUrl).toBe('https://pmc.ncbi.nlm.nih.gov/articles/PMC11169428/');
    expect(first.modality).toBe('CT');
    expect(first.title.length).toBeGreaterThan(0);
  });

  it('turns caption markup into plain text', async () => {
    const { images } = await new ImageSearchService(fakeFetch(OPENI)).search('appendectomy');
    const bleeding = images.find((i) => i.pmcid === 'PMC10625278');

    expect(bleeding?.caption).toMatch(/^Post-appendectomy bleeding/);
    expect(images.some((i) => /<|&[a-z]+;/.test(i.caption))).toBe(false);
  });

  it('asks Open-i for the type of image wanted, and for more figures than it shows', async () => {
    const requests: string[] = [];
    await new ImageSearchService(fakeFetch(OPENI, requests)).search('knee arthroplasty', { type: 'xray', limit: 10 });

    const params = new URL(requests[0]).searchParams;
    expect(params.get('query')).toBe('knee arthroplasty');
    expect(params.get('it')).toBe('x');
    expect(params.get('n')).toBe('40');
    expect(params.get('coll')).toBe('pmc');
  });

  it('searches for the clinical term of an everyday one', async () => {
    const requests: string[] = [];
    await new ImageSearchService(fakeFetch(OPENI, requests)).search('how is a heart attack treated');

    expect(new URL(requests[0]).searchParams.get('query')).toBe('myocardial infarction treated');
  });

  it('does not send a query about one patient', async () => {
    const requests: string[] = [];
    const result = await new ImageSearchService(fakeFetch(OPENI, requests)).search(
      'my patient John Smith, 45, has a rash on his arm'
    );

    expect(result).toEqual({ images: [], status: 'skipped' });
    expect(requests).toHaveLength(0);
  });

  it('says Open-i is unavailable instead of failing', async () => {
    const result = await new ImageSearchService(fakeFetch('', [], 503)).search('appendectomy');

    expect(result).toEqual({ images: [], status: 'unavailable' });
  });

  it('answers a repeated search from its cache', async () => {
    const requests: string[] = [];
    const service = new ImageSearchService(fakeFetch(OPENI, requests));
    await service.search('appendectomy');
    await service.search('Appendectomy ');

    expect(requests).toHaveLength(1);
  });
});
