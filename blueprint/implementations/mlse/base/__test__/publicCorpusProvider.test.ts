import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { PublicCorpusProvider } from '../services/publicCorpusProvider.service';

const openTelemetryCollector = new OpenTelemetryCollector('test', 'info', {});
const provider = new PublicCorpusProvider(openTelemetryCollector);

describe('PublicCorpusProvider', () => {
  it('serves the six version 1 sources', () => {
    expect(provider.describe().map((source) => source.id)).toEqual([
      'openfda',
      'dailymed',
      'clinicaltrials',
      'mesh',
      'pubmed',
      'pmc_oa'
    ]);
  });

  it('only includes sources that permit commercial use', () => {
    expect(provider.describe().every((source) => source.commercialUse)).toBe(
      true
    );
  });

  it('marks the sources MLSE queries live at search time', () => {
    const live = provider
      .describe()
      .filter((source) => source.liveQuery)
      .map((source) => source.id);
    expect(live).toEqual(['openfda', 'clinicaltrials', 'pubmed', 'pmc_oa']);
  });

  it('returns copies, so callers cannot change the shared list', () => {
    provider.describe()[0].name = 'changed';
    expect(provider.describe()[0].name).toBe('openFDA drug labels');
  });
});
