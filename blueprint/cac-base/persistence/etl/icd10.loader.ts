import { Readable } from 'node:stream';
import { Icd10Code } from '../entities/icd10Code.entity';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from './codeSetLoader.service';
import {
  CsvColumnMap,
  newParseStats,
  parseCodeThenDescriptionLines,
  parseCsvRows
} from './csvRowSource';

// CDC/NCHS publishes ICD-10-CM annually, effective October 1 — §7. With no
// column map the source is read as the CDC codes file itself
// (icd10cm_codes_YYYY.txt: code, spaces, description, no header); pass a
// column map for a delimited export instead.
export async function loadIcd10Codes(
  loader: CodeSetLoaderService,
  source: Readable,
  options: { columnMap?: CsvColumnMap; replaceRelease?: boolean } = {}
): Promise<CodeSetLoadResult> {
  const stats = newParseStats();
  const rows = options.columnMap
    ? parseCsvRows(source, options.columnMap, stats)
    : parseCodeThenDescriptionLines(source, stats);
  return loader.load(Icd10Code, rows, {
    tableName: 'icd10_code',
    replaceRelease: options.replaceRelease,
    skipped: () => stats.skipped
  });
}
