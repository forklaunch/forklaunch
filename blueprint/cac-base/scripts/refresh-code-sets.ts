import { createReadStream } from 'node:fs';
import { getEnvVar } from '@forklaunch/common';
import { ci, tokens } from '../bootstrapper';
import { CodeSetLoaderService } from '../persistence/etl/codeSetLoader.service';
import { loadCptCodes } from '../persistence/etl/cpt.loader';
import { loadHcpcsCodes } from '../persistence/etl/hcpcs.loader';
import { loadIcd10Codes } from '../persistence/etl/icd10.loader';

// Invoked externally on a schedule (k8s CronJob / cloud scheduler), same
// convention as scripts/enforce-retention.ts — there is no in-repo cron
// trigger (§7). Run at the tightest cadence of the code sets it refreshes
// (HCPCS/NCCI are quarterly).
//
// ICD10_SOURCE_PATH / HCPCS_SOURCE_PATH point at a local CSV file today —
// swapping this for a real feed later (a larger CMS/CDC release, an S3
// object) only means writing a new row source; the batching/upsert logic in
// CodeSetLoaderService and the loaders above stays the same. See §7.
//
// CPT_SOURCE_PATH is the real-CPT extension point (§5) — set only by an
// organization that has wired in their own licensed feed; ForkLaunch never
// sets this itself. See loadCptCodes / cpt.loader.ts.
async function main() {
  const orm = ci.resolve(tokens.Orm);
  const otel = ci.resolve(tokens.OtelCollector);

  try {
    const loader = new CodeSetLoaderService(orm.em, otel);

    const icd10SourcePath = getEnvVar('ICD10_SOURCE_PATH');
    const hcpcsSourcePath = getEnvVar('HCPCS_SOURCE_PATH');

    if (icd10SourcePath) {
      const result = await loadIcd10Codes(
        loader,
        createReadStream(icd10SourcePath, { encoding: 'utf-8' })
      );
      otel.info('[refresh-code-sets] ICD-10-CM refresh complete', result);
    } else {
      otel.warn(
        '[refresh-code-sets] ICD10_SOURCE_PATH not set — skipping ICD-10-CM refresh'
      );
    }

    if (hcpcsSourcePath) {
      const result = await loadHcpcsCodes(
        loader,
        createReadStream(hcpcsSourcePath, { encoding: 'utf-8' })
      );
      otel.info('[refresh-code-sets] HCPCS refresh complete', result);
    } else {
      otel.warn(
        '[refresh-code-sets] HCPCS_SOURCE_PATH not set — skipping HCPCS refresh'
      );
    }

    // Real CPT (§5) — only runs when an organization has actually pointed
    // this at their own licensed feed. Column positions are configurable
    // (no sane default, unlike ICD-10/HCPCS) because there's no one
    // standard file shape for a real CPT feed the way there is for
    // CDC/CMS releases.
    const cptSourcePath = getEnvVar('CPT_SOURCE_PATH');
    const cptOrganizationId = getEnvVar('CPT_ORGANIZATION_ID');

    if (cptSourcePath && cptOrganizationId) {
      const result = await loadCptCodes(
        loader,
        createReadStream(cptSourcePath, { encoding: 'utf-8' }),
        {
          code: Number(getEnvVar('CPT_CODE_COLUMN') ?? '0'),
          description: Number(getEnvVar('CPT_DESCRIPTION_COLUMN') ?? '1'),
          hasHeader: getEnvVar('CPT_HAS_HEADER') !== 'false'
        },
        cptOrganizationId
      );
      otel.info('[refresh-code-sets] CPT refresh complete', {
        organizationId: cptOrganizationId,
        ...result
      });
    } else if (cptSourcePath || cptOrganizationId) {
      otel.warn(
        '[refresh-code-sets] CPT_SOURCE_PATH and CPT_ORGANIZATION_ID must both be set — skipping CPT refresh'
      );
    } else {
      otel.warn(
        '[refresh-code-sets] CPT_SOURCE_PATH not set — skipping CPT refresh (expected until an organization wires in their own licensed feed, §5)'
      );
    }
  } finally {
    // Without this, the open Postgres connection pool keeps the process
    // alive indefinitely on the success path — a k8s CronJob/cloud
    // scheduler invocation of this script would never actually complete.
    await orm.close();
  }
}

main().catch((err) => {
  console.error('[refresh-code-sets] Fatal error', err);
  process.exit(1);
});
