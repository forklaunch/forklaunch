import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { EntityManager } from '@mikro-orm/postgresql';
import { WorklistStatus } from '../domain/enum/worklistStatus.enum';
import { Denial } from '../persistence/entities/denial.entity';

export interface DenialListFilter {
  claimId?: string;
  worklistStatus?: WorklistStatus;
}

// Backs the denial worklist API — the one piece of the old "eligibility &
// remittance" phase that never depended on Stedi (plan §12 item 12, §14).
// Every row here comes from ScrubbingService (§6); cac-base never submits
// claims or receives real remittances itself (§8), so this is purely a
// query/status layer over Denial rows the claim engine already creates.
export class DenialWorklistService {
  constructor(
    private readonly em: EntityManager,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async listDenials(
    organizationId: string,
    filter?: DenialListFilter
  ): Promise<Denial[]> {
    const where: {
      organizationId: string;
      claim?: string;
      worklistStatus?: WorklistStatus;
    } = { organizationId };
    if (filter?.claimId) where.claim = filter.claimId;
    if (filter?.worklistStatus) where.worklistStatus = filter.worklistStatus;

    const denials = await this.em.find(Denial, where, {
      orderBy: { createdAt: 'desc' }
    });
    this.otel.debug('Listed denials', { filter, count: denials.length });
    return denials;
  }

  async getDenial(organizationId: string, id: string): Promise<Denial | null> {
    return this.em.findOne(Denial, { id, organizationId });
  }

  async resolveDenial(
    organizationId: string,
    id: string
  ): Promise<Denial | null> {
    const denial = await this.em.findOne(Denial, { id, organizationId });
    if (!denial) return null;

    denial.worklistStatus = WorklistStatus.RESOLVED;
    denial.resolvedAt = new Date();
    await this.em.persist(denial).flush();

    this.otel.info('Resolved denial', { id });
    return denial;
  }
}
