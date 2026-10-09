import { FetchLike } from '../domain/http';
import { ImageSearchService } from '../services/imageSearch.service';
import { findStepFigures, stepFigureScore } from '../services/stepFigures.service';

const APPENDECTOMY = { names: ['appendectomy', 'appendicectomy'], hints: ['umbilical', 'port', 'incision', 'stump'] };
const photo = (caption: string, title = 'Two-port laparoscopic appendectomy') => ({ caption, title, modality: 'photo' });

describe('stepFigureScore', () => {
  it('accepts a photo whose caption describes a step of the procedure', () => {
    expect(stepFigureScore(photo('First sub umbilical trocar for accommodation of camera'), APPENDECTOMY)).toBeGreaterThan(0);
    expect(
      stepFigureScore(
        { caption: 'Off-pump anastomosis of LITA to LAD.', title: 'Robotic-assisted minimally invasive direct coronary artery bypass', modality: 'multi-panel figure', panels: ['photo'] },
        { names: ['coronary artery bypass', 'cabg'], hints: ['anastomosis', 'graft'] }
      )
    ).toBeGreaterThan(0);
  });

  it('names the procedure by its distinctive word', () => {
    expect(
      stepFigureScore(
        { caption: 'A Blunt versus B sharp expansion of uterine incision', title: 'Techniques at caesarean delivery', modality: 'photo' },
        { names: ['cesarean section', 'caesarean section'], hints: ['incision', 'uterine'] }
      )
    ).toBeGreaterThan(0);
  });

  it('rejects imaging, charts, animal studies and photos taken after the operation', () => {
    for (const caption of [
      'Mesh appearances on CT axial sections after port placement',
      'Ultrasound of the incision',
      'Flowchart of port placement in the study',
      'Port placement in a porcine model',
      'Incision healing status was observed 9 months after surgery.'
    ]) {
      expect(stepFigureScore(photo(caption), APPENDECTOMY)).toBeUndefined();
    }
    expect(stepFigureScore({ ...photo('Port placement'), modality: 'x-ray' }, APPENDECTOMY)).toBeUndefined();
  });

  it('rejects figures from articles on another procedure, or about another step', () => {
    expect(stepFigureScore(photo('Port placement', 'Single-port cholecystectomy'), APPENDECTOMY)).toBeUndefined();
    expect(stepFigureScore(photo('The mesoappendix was dissected'), APPENDECTOMY)).toBeUndefined();
  });

  it('ranks a figure in steps above a single view', () => {
    const single = stepFigureScore(photo('Umbilical port placed'), APPENDECTOMY) ?? 0;
    const steps = stepFigureScore(photo('(A) Umbilical port placed. (B) Second port inserted under vision.'), APPENDECTOMY) ?? 0;
    expect(steps).toBeGreaterThan(single);
  });
});

describe('findStepFigures', () => {
  const openI = (captions: string[], first = 100) =>
    JSON.stringify({
      list: captions.map((caption, i) => ({
        uid: `PMC${first + i}`,
        pmcid: String(first + i),
        title: 'Two-port laparoscopic appendectomy',
        licenseURL: 'https://creativecommons.org/licenses/by/4.0/',
        imgLarge: `/imgs/512/1/${first + i}.png`,
        image: { id: `F${i}`, caption, modalityMajor: 'ph' }
      }))
    });

  it('searches by the step and then by the technique, keeping only step figures', async () => {
    const requests: string[] = [];
    const fetchImpl: FetchLike = async (url) => {
      requests.push(url);
      const query = new URL(url).searchParams.get('query') ?? '';
      return {
        ok: true,
        status: 200,
        text: async () =>
          query.includes('umbilical')
            ? openI(['Umbilical port placed', 'Flowchart of patients'])
            : openI(['(A) Umbilical incision. (B) Second port inserted.'], 200)
      };
    };

    const figures = await findStepFigures(
      new ImageSearchService(fetchImpl),
      { title: 'Appendectomy', names: APPENDECTOMY.names },
      { hints: ['incision', 'port'], topicHints: ['umbilical', 'mcburney'] }
    );

    expect([...new Set(requests.map((url) => new URL(url).searchParams.get('query')))]).toEqual([
      'appendectomy umbilical mcburney technique',
      'appendectomy surgical technique'
    ]);
    expect(requests.every((url) => new URL(url).searchParams.get('it') === 'ph')).toBe(true);
    expect(figures.map((f) => f.caption)).toEqual(['(A) Umbilical incision. (B) Second port inserted.', 'Umbilical port placed']);
  });
});
