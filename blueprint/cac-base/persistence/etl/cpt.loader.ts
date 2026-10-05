import { Readable } from 'node:stream';
import { CptCode } from '../entities/cptCode.entity';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from './codeSetLoader.service';
import { CsvColumnMap, newParseStats, parseCsvRows } from './csvRowSource';

// Points at an organization's own real, licensed CPT feed (§5) — never a
// ForkLaunch-supplied source. The column layout is left to the caller to
// specify explicitly (no DEFAULT_COLUMN_MAP, unlike ICD-10/HCPCS): unlike
// the free code sets, there's no one standard file shape here — every
// organization's real feed may look different, and this loader stays
// agnostic to that on purpose (§5 readiness bar, item 3).
export async function loadCptCodes(
  loader: CodeSetLoaderService,
  source: Readable,
  columnMap: CsvColumnMap,
  organizationId: string,
  options: { replaceRelease?: boolean } = {}
): Promise<CodeSetLoadResult> {
  const stats = newParseStats();
  return loader.load(CptCode, parseCsvRows(source, columnMap, stats), {
    tableName: 'cpt_code',
    onConflictFields: ['organizationId', 'code'],
    organizationId,
    replaceRelease: options.replaceRelease,
    skipped: () => stats.skipped
  });
}
