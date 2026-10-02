import { FetchLike } from '../domain/http';
import {
  editDistance,
  localCompletions,
  localCorrection,
  onlyWordForms,
  QuerySuggestionService
} from '../services/querySuggestions.service';

const ncbi = { tool: 'mlse-test', email: 'dev@example.com' };

// recorded NLM responses, keyed by a part of the URL
function fakeFetch(responses: Record<string, string>, requests: string[] = []): FetchLike {
  return async (url) => {
    requests.push(url);
    const key = Object.keys(responses).find((k) => url.includes(k));
    return { ok: true, status: 200, text: async () => (key ? responses[key] : '[0,[],null,[]]') };
  };
}

const MEDLINEPLUS_NO_RESULTS = (correction: string) =>
  `<?xml version="1.0"?><nlmSearchResult><count>0</count>${correction ? `<spellingCorrection>${correction}</spellingCorrection>` : ''}<list num="0"/></nlmSearchResult>`;

describe('editDistance', () => {
  it('counts a swap of neighbouring letters as one edit', () => {
    expect(editDistance('atatck', 'attack', 2)).toBe(1);
    expect(editDistance('heart atack', 'heart attack', 2)).toBe(1);
    expect(editDistance('stroke', 'hemorrhoids', 2)).toBe(3);
  });
});

describe('localCorrection', () => {
  it('fixes misspelled known terms and keeps the rest of the query', () => {
    expect(localCorrection('heart atack')).toBe('heart attack');
    expect(localCorrection('Heart atack symptoms')).toBe('heart attack symptoms');
    expect(localCorrection('high blood presure in pregnancy')).toBe('high blood pressure in pregnancy');
    expect(localCorrection('heartatack')).toBe('heart attack');
    expect(localCorrection('heartattack')).toBe('heart attack');
    expect(localCorrection('myocardail infraction')).toBe('myocardial infarction');
  });

  it('leaves correct spellings, other word forms and short words alone', () => {
    expect(localCorrection('heart attack')).toBeUndefined();
    expect(localCorrection('hypotension after spinal anesthesia')).toBeUndefined();
    expect(localCorrection('hypertensive emergency')).toBeUndefined();
    expect(localCorrection('kidney stones')).toBeUndefined();
    expect(localCorrection('infection after surgery')).toBeUndefined();
    expect(localCorrection('flue')).toBeUndefined();
  });
});

describe('onlyWordForms', () => {
  it('tells a change of word form from a spelling fix', () => {
    expect(onlyWordForms('laparoscopic cholecystectomy', 'laparoscopy cholecystectomy')).toBe(true);
    expect(onlyWordForms('kidney stone', 'kidney stones')).toBe(true);
    expect(onlyWordForms('diabetis', 'diabetes')).toBe(false);
    expect(onlyWordForms('pnuemonia', 'pneumonia')).toBe(false);
  });
});

describe('localCompletions', () => {
  it('completes known terms from what has been typed', () => {
    expect(localCompletions('heart at')).toEqual(['heart attack']);
    expect(localCompletions('h')).toEqual([]);
  });
});

describe('QuerySuggestionService', () => {
  it('asks MedlinePlus, then NCBI, for a spelling the local list does not know', async () => {
    const requests: string[] = [];
    const service = new QuerySuggestionService(
      fakeFetch(
        {
          'espell.fcgi': '<eSpellResult><CorrectedQuery>pneumonia</CorrectedQuery></eSpellResult>',
          'term=diabetis': MEDLINEPLUS_NO_RESULTS('diabetes'),
          'term=pnuemonia': MEDLINEPLUS_NO_RESULTS('')
        },
        requests
      ),
      ncbi
    );
    expect(await service.correct('diabetis')).toBe('diabetes');
    expect(await service.correct('pnuemonia')).toBe('pneumonia');
    expect(requests.filter((u) => u.includes('espell.fcgi'))).toHaveLength(1);
  });

  it('suggests nothing for a query that is spelled right', async () => {
    const service = new QuerySuggestionService(
      fakeFetch({ 'espell.fcgi': '<eSpellResult><CorrectedQuery></CorrectedQuery></eSpellResult>', healthTopics: MEDLINEPLUS_NO_RESULTS('') }),
      ncbi
    );
    expect(await service.correct('laparoscopic cholecystectomy')).toBeUndefined();
  });

  it('ignores a suggestion that only changes word forms', async () => {
    const service = new QuerySuggestionService(
      fakeFetch({
        'espell.fcgi': '<eSpellResult><CorrectedQuery>laparoscopy cholecystectomy</CorrectedQuery></eSpellResult>',
        healthTopics: MEDLINEPLUS_NO_RESULTS('')
      }),
      ncbi
    );
    expect(await service.correct('laparoscopic cholecystectomy')).toBeUndefined();
  });

  it('does not look up a known term spelled right', async () => {
    const requests: string[] = [];
    const service = new QuerySuggestionService(fakeFetch({}, requests), ncbi);
    expect(await service.correct('Heart attack')).toBeUndefined();
    expect(requests).toEqual([]);
  });

  it('completes conditions, procedures and medicines, names that start with the text first', async () => {
    const service = new QuerySuggestionService(
      fakeFetch({
        'conditions/v3': JSON.stringify([2, ['1', '2'], null, [['Three vessel coronary artery disease'], ['Heart attack (myocardial infarction)']]]),
        'rxterms/v3': JSON.stringify([1, ['x'], null, [['metFORMIN XR (Oral Pill)']]])
      }),
      ncbi
    );
    // names with every word typed; "Three vessel coronary artery disease"
    // is an NLM synonym match and is left out
    expect(await service.complete('heart at')).toEqual(['Heart attack (myocardial infarction)']);
    expect(await service.complete('metf')).toEqual(['metformin xr']);
  });

  it('never sends text about a patient, or with numbers, to NLM', async () => {
    const requests: string[] = [];
    const service = new QuerySuggestionService(fakeFetch({}, requests), ncbi);
    expect(await service.complete('heart attack 45 year old')).toEqual([]);
    expect(await service.correct('my patient has a heart atack')).toBe('my patient has a heart attack');
    expect(requests).toEqual([]);
  });

  it('falls back to the local list when NLM is slow', async () => {
    const service = new QuerySuggestionService(() => new Promise(() => undefined), ncbi, { timeoutMs: 20 });
    expect(await service.complete('heart at')).toEqual(['heart attack']);
    expect(await service.correct('heart atack')).toBe('heart attack');
  });
});
