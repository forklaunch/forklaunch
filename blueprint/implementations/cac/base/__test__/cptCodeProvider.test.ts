import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { CptCodeProvider, CptCodeSource } from '../services/cptCodeProvider.service';

const openTelemetryCollector = new OpenTelemetryCollector('test', 'info', {});

// A fake in-memory CptCodeSource, standing in for whatever real, licensed
// data source an adopting organization would supply (a DB table, a vendor
// API client, etc.) — proves the adapter shape works against *any* source,
// not tied to one backing implementation. Contains no real AMA content.
class FakeCptCodeSource implements CptCodeSource {
  constructor(private readonly codes: Record<string, string>) {}

  async lookup(code: string) {
    const description = this.codes[code];
    return description ? { code, description } : undefined;
  }
}

describe('CptCodeProvider', () => {
  it('describes itself as the real, licensed code set', () => {
    const provider = new CptCodeProvider(
      new FakeCptCodeSource({}),
      openTelemetryCollector
    );

    expect(provider.describe()).toEqual({
      codeSetType: 'cpt',
      licensed: true
    });
  });

  it('resolves a code from whatever source it is given', async () => {
    const provider = new CptCodeProvider(
      new FakeCptCodeSource({ '10001': 'Synthetic surgery-range code' }),
      openTelemetryCollector
    );

    const result = await provider.lookupProcedureCode({ code: '10001' });
    expect(result).toEqual({
      code: '10001',
      description: 'Synthetic surgery-range code'
    });
  });

  it('returns undefined for a code the source does not know', async () => {
    const provider = new CptCodeProvider(
      new FakeCptCodeSource({}),
      openTelemetryCollector
    );

    const result = await provider.lookupProcedureCode({ code: '99999' });
    expect(result).toBeUndefined();
  });
});
