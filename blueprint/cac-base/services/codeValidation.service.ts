import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { EntityManager } from '@mikro-orm/postgresql';
import { HcpcsCode } from '../persistence/entities/hcpcsCode.entity';
import { Icd10Code } from '../persistence/entities/icd10Code.entity';

export type CodeValidationResult =
  | { valid: true; code: string; description: string }
  | { valid: false; code: string };

// Backs the free-code-set validation endpoints (§10's forklaunch-platform
// UI is the eventual caller) — looks codes up against the reference tables
// populated by scripts/refresh-code-sets.ts (§7). No license, no real CPT
// content involved — ICD-10-CM and HCPCS only.
export class CodeValidationService {
  constructor(
    private readonly em: EntityManager,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async validateIcd10(code: string): Promise<CodeValidationResult> {
    const found = await this.em.findOne(Icd10Code, { code });
    this.otel.debug('Validated ICD-10-CM code', { code, found: !!found });
    return found
      ? { valid: true, code, description: found.description }
      : { valid: false, code };
  }

  async validateHcpcs(code: string): Promise<CodeValidationResult> {
    const found = await this.em.findOne(HcpcsCode, { code });
    this.otel.debug('Validated HCPCS code', { code, found: !!found });
    return found
      ? { valid: true, code, description: found.description }
      : { valid: false, code };
  }
}
