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

// The real-CPT extension point. This adapter is complete and
// production-shaped — same rigor as MockProcedureCodeProvider — but it never
// contains, ships, or fetches any real AMA CPT content. ForkLaunch cannot
// legally hold that content (§2); the adopting organization supplies it
// through CptCodeSource, backed by their own licensed data feed (a DB table
// populated via refresh-code-sets.ts, a vendor API client, whatever their
// real feed looks like — see plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §5).
//
// cac-base wires this up with an EntityManager-backed CptCodeSource against
// the CptCode reference table (persistence/entities/cptCode.entity.ts) —
// that's the concrete, ready-to-use starting point. Swap the source for
// something else (a vendor API client, a different table) without touching
// this class at all.
export interface CptCodeSource {
  lookup(code: string): Promise<ProcedureCodeDto | undefined>;
}

export class CptCodeProvider implements CodeSetProvider {
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  private readonly source: CptCodeSource;

  constructor(
    source: CptCodeSource,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>
  ) {
    this.source = source;
    this.openTelemetryCollector = openTelemetryCollector;
  }

  async lookupProcedureCode({
    code
  }: ProcedureCodeLookupDto): Promise<ProcedureCodeDto | undefined> {
    this.openTelemetryCollector.debug('Looking up real CPT procedure code', {
      code
    });

    return this.source.lookup(code);
  }

  describe(): CodeSetDescriptorDto {
    return { codeSetType: 'cpt', licensed: true };
  }
}
