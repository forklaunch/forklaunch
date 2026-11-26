import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { CodeSetProvider } from '@forklaunch/interfaces-cac/interfaces';
import {
  CodeSetDescriptorDto,
  ProcedureCodeDto,
  ProcedureCodeLookupDto
} from '@forklaunch/interfaces-cac/types';
import { normalizeCode } from '../domain/codeNormalization';
import { MOCK_PROCEDURE_CODES } from '../domain/mockProcedureCodes';

export class MockProcedureCodeProvider implements CodeSetProvider {
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;

  constructor(
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>
  ) {
    this.openTelemetryCollector = openTelemetryCollector;
  }

  async lookupProcedureCode({
    code
  }: ProcedureCodeLookupDto): Promise<ProcedureCodeDto | undefined> {
    this.openTelemetryCollector.debug('Looking up mock procedure code', {
      code
    });

    const normalized = normalizeCode(code);
    return Object.hasOwn(MOCK_PROCEDURE_CODES, normalized)
      ? MOCK_PROCEDURE_CODES[normalized]
      : undefined;
  }

  describe(): CodeSetDescriptorDto {
    return { codeSetType: 'mock', licensed: false };
  }
}
