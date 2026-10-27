import { Readable } from 'node:stream';
import { HcpcsCode } from '../entities/hcpcsCode.entity';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from './codeSetLoader.service';
import { CsvColumnMap, newParseStats, parseCsvRows } from './csvRowSource';

// CMS publishes HCPCS Level II quarterly (Jan/Apr/Jul/Oct) — §7. Same
// loader shape as ICD-10-CM (loadIcd10Codes) — HCPCS just has a tighter
// refresh cadence. The default map reads a comma-delimited export with a
// header row (code, description); CMS's own release is a spreadsheet, so
// export it to CSV or pass the map that matches your file.
const DEFAULT_COLUMN_MAP: CsvColumnMap = {
  code: 0,
  description: 1,
  hasHeader: true
};

export async function loadHcpcsCodes(
  loader: CodeSetLoaderService,
  source: Readable,
  options: { columnMap?: CsvColumnMap; replaceRelease?: boolean } = {}
): Promise<CodeSetLoadResult> {
  const stats = newParseStats();
  return loader.load(
    HcpcsCode,
    parseCsvRows(source, options.columnMap ?? DEFAULT_COLUMN_MAP, stats),
    {
      tableName: 'hcpcs_code',
      replaceRelease: options.replaceRelease,
      skipped: () => stats.skipped
    }
  );
}
