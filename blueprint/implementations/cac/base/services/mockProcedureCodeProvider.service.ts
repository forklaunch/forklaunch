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

    return MOCK_PROCEDURE_CODES[code];
  }

  describe(): CodeSetDescriptorDto {
    return { codeSetType: 'mock', licensed: false };
  }
}
