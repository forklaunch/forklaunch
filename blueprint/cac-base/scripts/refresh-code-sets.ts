import { createReadStream } from 'node:fs';
import { getEnvVar } from '@forklaunch/common';
import { ci, tokens } from '../bootstrapper';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from '../persistence/etl/codeSetLoader.service';
import { loadCptCodes } from '../persistence/etl/cpt.loader';
import { loadHcpcsCodes } from '../persistence/etl/hcpcs.loader';
import { columnIndexFromEnv, flagFromEnv } from '../persistence/etl/env';
import { loadIcd10Codes } from '../persistence/etl/icd10.loader';

// Invoked externally on a schedule (k8s CronJob / cloud scheduler), same
// convention as scripts/enforce-retention.ts — there is no in-repo cron
// trigger (§7). Run at the tightest cadence of the code sets it refreshes
// (HCPCS/NCCI are quarterly).
//
// ICD10_SOURCE_PATH is the CDC codes file as published
// (icd10cm_codes_YYYY.txt); HCPCS_SOURCE_PATH is a CSV export of the CMS
// release (code, description, header row). Each is a complete release, so
// codes it no longer contains are removed (CODE_SET_REPLACE_RELEASE=false
// keeps them). Swapping either for another feed later (an S3 object) only
// means writing a new row source; the batching/upsert logic in
// CodeSetLoaderService and the loaders above stays the same. See §7.
//
// A line the parser cannot use fails the run (exit code 1) after loading
// the rest, so a scheduler reports a malformed file instead of a silent
// partial load.
//
// CPT_SOURCE_PATH is the real-CPT extension point (§5) — set only by an
// organization that has wired in their own licensed feed; ForkLaunch never
// sets this itself. See loadCptCodes / cpt.loader.ts.
async function main() {
  const orm = ci.resolve(tokens.Orm);
  const otel = ci.resolve(tokens.OtelCollector);
  const problems: string[] = [];
  const check = (codeSet: string, result: CodeSetLoadResult) => {
    if (result.rowsSkipped > 0) {
      problems.push(`${codeSet}: ${result.rowsSkipped} line(s) had no usable code and description`);
    }
  };

  try {
    const loader = new CodeSetLoaderService(orm.em, otel);
    const replaceRelease = flagFromEnv(
      'CODE_SET_REPLACE_RELEASE',
      getEnvVar('CODE_SET_REPLACE_RELEASE'),
      true
    );

    const icd10SourcePath = getEnvVar('ICD10_SOURCE_PATH');
    const hcpcsSourcePath = getEnvVar('HCPCS_SOURCE_PATH');

    if (icd10SourcePath) {
      const result = await loadIcd10Codes(
        loader,
        createReadStream(icd10SourcePath, { encoding: 'utf-8' }),
        { replaceRelease }
      );
      otel.info('[refresh-code-sets] ICD-10-CM refresh complete', result);
      check('ICD-10-CM', result);
    } else {
      otel.warn(
        '[refresh-code-sets] ICD10_SOURCE_PATH not set — skipping ICD-10-CM refresh'
      );
    }

    if (hcpcsSourcePath) {
      const result = await loadHcpcsCodes(
        loader,
        createReadStream(hcpcsSourcePath, { encoding: 'utf-8' }),
        { replaceRelease }
      );
      otel.info('[refresh-code-sets] HCPCS refresh complete', result);
      check('HCPCS', result);
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
          code: columnIndexFromEnv('CPT_CODE_COLUMN', getEnvVar('CPT_CODE_COLUMN'), 0),
          description: columnIndexFromEnv(
            'CPT_DESCRIPTION_COLUMN',
            getEnvVar('CPT_DESCRIPTION_COLUMN'),
            1
          ),
          hasHeader: flagFromEnv('CPT_HAS_HEADER', getEnvVar('CPT_HAS_HEADER'), true)
        },
        cptOrganizationId,
        // an organization's feed may be partial, so CPT codes are only
        // removed when it says the file is a full release
        {
          replaceRelease: flagFromEnv(
            'CPT_REPLACE_RELEASE',
            getEnvVar('CPT_REPLACE_RELEASE'),
            false
          )
        }
      );
      otel.info('[refresh-code-sets] CPT refresh complete', {
        organizationId: cptOrganizationId,
        ...result
      });
      check('CPT', result);
    } else if (cptSourcePath || cptOrganizationId) {
      otel.warn(
        '[refresh-code-sets] CPT_SOURCE_PATH and CPT_ORGANIZATION_ID must both be set — skipping CPT refresh'
      );
    } else {
      otel.warn(
        '[refresh-code-sets] CPT_SOURCE_PATH not set — skipping CPT refresh (expected until an organization wires in their own licensed feed, §5)'
      );
    }
    if (problems.length > 0) {
      throw new Error(`Code-set refresh loaded with problems: ${problems.join('; ')}`);
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
