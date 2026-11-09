import { Readable } from 'node:stream';
import { HcpcsCode } from '../entities/hcpcsCode.entity';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from './codeSetLoader.service';
import {
  CsvColumnMap,
  newParseStats,
  parseCsvRows,
  parseHcpcsReleaseRows
} from './csvRowSource';

// CMS publishes HCPCS Level II quarterly (Jan/Apr/Jul/Oct) — §7. Same
// loader shape as ICD-10-CM (loadIcd10Codes) — HCPCS just has a tighter
// refresh cadence. CMS ships the release as a spreadsheet; export it to CSV
// and it is read as published (HCPC, SEQNUM, RECID, LONG DESCRIPTION...,
// see parseHcpcsReleaseRows): continuation rows are joined and modifier
// rows left out. A plain code,description CSV with a header row also
// works. Pass a columnMap to read any other layout.
export async function loadHcpcsCodes(
  loader: CodeSetLoaderService,
  source: Readable,
  options: { columnMap?: CsvColumnMap; replaceRelease?: boolean } = {}
): Promise<CodeSetLoadResult> {
  const stats = newParseStats();
  return loader.load(
    HcpcsCode,
    options.columnMap
      ? parseCsvRows(source, options.columnMap, stats)
      : parseHcpcsReleaseRows(source, stats),
    {
      tableName: 'hcpcs_code',
      replaceRelease: options.replaceRelease,
      skipped: () => stats.skipped
    }
  );
}
