import { Readable } from 'node:stream';
import { HcpcsCode } from '../entities/hcpcsCode.entity';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from './codeSetLoader.service';
import { CsvColumnMap, parseCsvRows } from './csvRowSource';

// CMS publishes HCPCS Level II quarterly (Jan/Apr/Jul/Oct) — §7. Same
// loader shape as ICD-10-CM (loadIcd10Codes) — HCPCS just has a tighter
// refresh cadence.
const DEFAULT_COLUMN_MAP: CsvColumnMap = {
  code: 0,
  description: 1,
  hasHeader: true
};

export async function loadHcpcsCodes(
  loader: CodeSetLoaderService,
  source: Readable,
  columnMap: CsvColumnMap = DEFAULT_COLUMN_MAP
): Promise<CodeSetLoadResult> {
  return loader.load(HcpcsCode, parseCsvRows(source, columnMap));
}
