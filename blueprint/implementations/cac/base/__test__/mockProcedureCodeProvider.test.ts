import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { MockProcedureCodeProvider } from '../services/mockProcedureCodeProvider.service';

const openTelemetryCollector = new OpenTelemetryCollector('test', 'info', {});
const provider = new MockProcedureCodeProvider(openTelemetryCollector);

describe('MockProcedureCodeProvider', () => {
  it('describes itself as the unlicensed mock code set', () => {
    expect(provider.describe()).toEqual({
      codeSetType: 'mock',
      licensed: false
    });
  });

  it('resolves a known mock procedure code', async () => {
    const result = await provider.lookupProcedureCode({ code: 'PROC-001' });
    expect(result).toEqual({ code: 'PROC-001', description: 'Office Visit' });
  });

  it('returns undefined for an unknown code', async () => {
    const result = await provider.lookupProcedureCode({ code: 'PROC-999' });
    expect(result).toBeUndefined();
  });
});
