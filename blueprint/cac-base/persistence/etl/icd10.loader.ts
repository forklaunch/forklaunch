import { Readable } from 'node:stream';
import { Icd10Code } from '../entities/icd10Code.entity';
import {
  CodeSetLoaderService,
  CodeSetLoadResult
} from './codeSetLoader.service';
import { CsvColumnMap, parseCsvRows } from './csvRowSource';

// CDC/NCHS publishes ICD-10-CM annually, effective October 1 — §7.
const DEFAULT_COLUMN_MAP: CsvColumnMap = {
  code: 0,
  description: 1,
  hasHeader: true
};

export async function loadIcd10Codes(
  loader: CodeSetLoaderService,
  source: Readable,
  columnMap: CsvColumnMap = DEFAULT_COLUMN_MAP
): Promise<CodeSetLoadResult> {
  return loader.load(Icd10Code, parseCsvRows(source, columnMap));
}
