import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import {
  CodeSetProvider,
  CptCodeProvider,
  MockProcedureCodeProvider
} from '@forklaunch/implementation-cac-base/services';
import { EntityManager } from '@mikro-orm/postgresql';
import { CodeSetType } from '../domain/enum/codeSetType.enum';
import { LicenseStatus } from '../domain/enum/licenseStatus.enum';
import { CodeSetLicense } from '../persistence/entities/codeSetLicense.entity';
import { EntityManagerCptCodeSource } from './cptCodeSource.service';

// Per-organization runtime feature gate (§5, plan §12 item 11). Not built
// on requiredFeatures/hasFeatureChecks: that mechanism *blocks* a route
// entirely when a feature is missing (right for billing's "you need this
// plan to access this endpoint"), but real CPT is never a hard
// requirement here — an org without it should transparently keep getting
// mock data, not a 403. So this is a plain resolve-and-branch: look up
// whether the organization's CodeSetLicense is active, and hand back
// whichever CodeSetProvider it should use, with the interface itself
// (MockProcedureCodeProvider / CptCodeProvider) completely unchanged.
//
// describe()/lookupProcedureCode() on CodeSetProvider are deliberately
// not async-license-check-aware themselves — that would mean rewriting the
// interface every mock/real implementation follows. Instead, callers
// (codeSet.controller.ts) resolve the right provider first, then call the
// unmodified interface on whatever they got back.
export class CodeSetProviderResolver {
  constructor(
    private readonly em: EntityManager,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async resolve(organizationId: string | undefined): Promise<CodeSetProvider> {
    const mockProvider = new MockProcedureCodeProvider(this.otel);

    if (!organizationId) {
      return mockProvider;
    }

    const license = await this.em.findOne(CodeSetLicense, {
      organizationId,
      codeSetType: CodeSetType.CPT,
      status: LicenseStatus.ACTIVE
    });

    if (!license) {
      return mockProvider;
    }

    this.otel.debug('Resolved real-CPT provider for organization', {
      organizationId
    });
    return new CptCodeProvider(
      new EntityManagerCptCodeSource(this.em, organizationId),
      this.otel
    );
  }
}
