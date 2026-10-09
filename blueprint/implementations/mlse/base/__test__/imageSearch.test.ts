import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { FetchLike } from '../domain/http';
import { ImageSearchService, isChart, rankByCaption } from '../services/imageSearch.service';

const OPENI = readFileSync(join(__dirname, 'fixtures', 'openi-search.json'), 'utf8');

function fakeFetch(body: string, requests: string[] = [], status = 200): FetchLike {
  return async (url) => {
    requests.push(url);
    return { ok: status === 200, status, text: async () => body };
  };
}

describe('ImageSearchService', () => {
  it('returns only figures licensed for commercial reuse, with their article and license', async () => {
    const { images, status } = await new ImageSearchService(fakeFetch(OPENI)).search('appendectomy', { charts: true });

    expect(status).toBe('ok');
    // of 12 figures: 5 unlicensed, 3 NonCommercial or NoDerivatives
    expect(images.map((i) => i.pmcid)).toEqual(['PMC11169428', 'PMC7486639', 'PMC10625278', 'PMC12263465']);
    expect(images.every((i) => i.license === 'CC BY 4.0')).toBe(true);
    const [first] = images;
    expect(first.imageUrl).toMatch(/^https:\/\/openi\.nlm\.nih\.gov\/imgs\/512\//);
    expect(first.thumbnailUrl).toMatch(/^https:\/\/openi\.nlm\.nih\.gov\/imgs\/150\//);
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

    const pages = requests.map((url) => new URL(url).searchParams);
    expect(pages.map((p) => [p.get('m'), p.get('n')])).toEqual([['1', '30'], ['31', '40']]);
    expect(pages.every((p) => p.get('query') === 'knee arthroplasty' && p.get('it') === 'x' && p.get('coll') === 'pmc')).toBe(true);
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
    const requests: string[] = [];
    const result = await new ImageSearchService(fakeFetch('', requests, 503)).search('appendectomy');

    expect(result).toEqual({ images: [], status: 'unavailable' });
    // three pages, each asked twice
    expect(requests).toHaveLength(6);
  });

  it('asks again once when Open-i fails a request', async () => {
    let calls = 0;
    const flaky: FetchLike = async () => {
      calls++;
      return calls <= 2
        ? { ok: false, status: 502, text: async () => '' }
        : { ok: true, status: 200, text: async () => OPENI };
    };
    const result = await new ImageSearchService(flaky).search('appendectomy');

    expect(result.status).toBe('ok');
    expect(result.images.length).toBeGreaterThan(0);
  });

  it('answers a repeated search from its cache', async () => {
    const requests: string[] = [];
    const service = new ImageSearchService(fakeFetch(OPENI, requests));
    await service.search('appendectomy');
    const asked = requests.length;
    await service.search('Appendectomy ');

    expect(requests).toHaveLength(asked);
  });
});

describe('rankByCaption', () => {
  it('keeps only figures whose caption names the topic', () => {
    const ranked = rankByCaption(
      [
        { caption: 'Immunofluorescent staining of hippocampal microglia', title: 'Neuroinflammation in aged mice' },
        { caption: 'Mice 4 weeks after the operation', title: 'Appendectomy and colitis in mice' },
        { caption: 'The appendix delivered through the incision', title: 'Open appendectomy in children' },
        { caption: 'Appendectomy specimen', title: 'A rare tumour' }
      ],
      'appendectomy'
    );

    // "appendix" is a form of "appendectomy"'s word; ties keep Open-i's order
    expect(ranked.map((r) => r.caption)).toEqual(['The appendix delivered through the incision', 'Appendectomy specimen']);
  });

  it('with orTitle, also keeps figures whose article title names the topic, after the rest', () => {
    const ranked = rankByCaption(
      [
        { caption: 'Mice 4 weeks after the operation', title: 'Appendectomy and colitis in mice' },
        { caption: 'Appendectomy specimen', title: 'A rare tumour' },
        { caption: 'Immunofluorescent staining of hippocampal microglia', title: 'Neuroinflammation in aged mice' }
      ],
      'appendectomy',
      { orTitle: true }
    );

    expect(ranked.map((r) => r.caption)).toEqual(['Appendectomy specimen', 'Mice 4 weeks after the operation']);
  });

  it('counts word forms as one word', () => {
    const [only] = rankByCaption(
      [{ caption: 'ECG of an acute anterior myocardial infarct', title: 'Case report' }],
      'myocardial infarction'
    );
    expect(only).toBeDefined();
  });
});

describe('charts', () => {
  it('tells charts and diagrams from images by caption', () => {
    for (const caption of [
      'Flow diagram (alluvial plot) illustrating the frequency of cause of death',
      'Kinetics of high-sensitivity cardiac troponin I concentration from symptom onset',
      'Study population and classification of stroke subtypes',
      'Overview of the method and its potential therapeutic approach'
    ]) {
      expect(isChart(caption)).toBe(true);
    }
    for (const caption of [
      'An example of 1 mm elevation of ST segment in II, III, aVF leads',
      'Case of spontaneous myocardial infarction (A) on MRI',
      'Sigma High Performance unicompartmental knee replacement and patello-femoral joint implants',
      'Appendectomy specimen'
    ]) {
      expect(isChart(caption)).toBe(false);
    }
  });

  it('leaves charts out unless asked for them or for diagrams', async () => {
    const body = JSON.stringify({
      list: ['Appendectomy specimen', 'Flowchart of appendectomy patients'].map((caption, i) => ({
        uid: `PMC${300 + i}`,
        pmcid: String(300 + i),
        title: 'Appendectomy in adults',
        licenseURL: 'https://creativecommons.org/licenses/by/4.0/',
        imgLarge: `/imgs/512/1/${300 + i}.png`,
        image: { id: 'F1', caption, modalityMajor: 'ph' }
      }))
    });
    const service = new ImageSearchService(fakeFetch(body));
    const captions = async (options: { type?: 'diagram'; charts?: boolean }) =>
      (await service.search('appendectomy', options)).images.map((i) => i.caption);

    expect(await captions({})).toEqual(['Appendectomy specimen']);
    expect(await captions({ type: 'diagram' })).toEqual(['Appendectomy specimen', 'Flowchart of appendectomy patients']);
    expect(await captions({ charts: true })).toHaveLength(2);
  });
});
