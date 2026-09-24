import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { EntityManager, raw } from '@mikro-orm/postgresql';
import { ClaimStatus } from '../domain/enum/claimStatus.enum';
import { Claim } from '../persistence/entities/claim.entity';
import { Denial } from '../persistence/entities/denial.entity';

export interface AnalyticsDateRange {
  since?: Date;
  until?: Date;
}

export interface ClaimAnalyticsSummary {
  totalScrubbedClaims: number;
  // 0-100. Complementary by construction: every scrubbed claim ends up
  // READY (clean) or DENIED (§6) — there's no partial-clean state.
  cleanClaimRate: number;
  denialRate: number;
  denialsByCategory: Record<string, number>;
}

// Reports the two of §11's three success metrics we can actually compute:
// clean-claim-rate and denial-rate, both derived from Claim/Denial data
// cac-base owns outright. Average-days-to-payment is dropped — it needs
// real remittance timing, which is out of scope entirely now (§8, §14),
// not just deferred. See plan §12 (RBAC/analytics scope note, §14 PR 5).
export class AnalyticsService {
  constructor(
    private readonly em: EntityManager,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async getClaimSummary(
    organizationId: string,
    range?: AnalyticsDateRange
  ): Promise<ClaimAnalyticsSummary> {
    const createdAtFilter = buildDateFilter(range);

    const [readyCount, deniedCount, categoryCounts] = await Promise.all([
      this.em.count(Claim, {
        organizationId,
        status: ClaimStatus.READY,
        ...(createdAtFilter ? { createdAt: createdAtFilter } : {})
      }),
      this.em.count(Claim, {
        organizationId,
        status: ClaimStatus.DENIED,
        ...(createdAtFilter ? { createdAt: createdAtFilter } : {})
      }),
      // Grouped/counted in the DB, not hydrated row-by-row — an
      // organization's full denial history can run far larger than any one
      // summary request needs to pull into memory just to tally categories.
      // Filtered by the parent claim's createdAt, not the denial's own —
      // a claim can be built on one day and scrubbed (denial created) on
      // another, and this must stay in the same date window as
      // readyCount/deniedCount above or denialsByCategory silently drifts
      // out of sync with deniedCount.
      this.em
        .createQueryBuilder(Denial, 'd')
        .select(['d.category', raw('count(*) as count')])
        .where({
          organizationId,
          ...(createdAtFilter ? { claim: { createdAt: createdAtFilter } } : {})
        })
        .groupBy('d.category')
        .execute<{ category: string; count: string }[]>('all', false)
    ]);

    const totalScrubbedClaims = readyCount + deniedCount;
    const cleanClaimRate =
      totalScrubbedClaims === 0 ? 0 : (readyCount / totalScrubbedClaims) * 100;
    const denialRate =
      totalScrubbedClaims === 0 ? 0 : (deniedCount / totalScrubbedClaims) * 100;

    // Postgres COUNT(*) comes back as a string (bigint) from the driver.
    const denialsByCategory: Record<string, number> = {};
    for (const row of categoryCounts) {
      denialsByCategory[row.category] = Number(row.count);
    }

    const summary: ClaimAnalyticsSummary = {
      totalScrubbedClaims,
      cleanClaimRate,
      denialRate,
      denialsByCategory
    };
    this.otel.debug('Computed claim analytics summary', summary);
    return summary;
  }
}

function buildDateFilter(
  range?: AnalyticsDateRange
): { $gte?: Date; $lte?: Date } | undefined {
  if (!range?.since && !range?.until) return undefined;
  const filter: { $gte?: Date; $lte?: Date } = {};
  if (range.since) filter.$gte = range.since;
  if (range.until) filter.$lte = range.until;
  return filter;
}
