import { createInterface } from 'node:readline';
import { Readable } from 'node:stream';
import { CodeSetRow } from './codeSetLoader.service';

export interface CsvColumnMap {
  /** 0-indexed column position of the code. */
  code: number;
  /** 0-indexed column position of the description. */
  description: number;
  /** 0-indexed column position of an optional effective-date column. */
  effectiveDate?: number;
  /** Column delimiter. Defaults to ','. */
  delimiter?: string;
  /** Skip the first line (header row). Defaults to true. */
  hasHeader?: boolean;
}

/** Lines a row source could not use, so a bad file is not loaded silently. */
export interface ParseStats {
  skipped: number;
  // the first few skipped line numbers, for the error message
  skippedLines: number[];
}

export function newParseStats(): ParseStats {
  return { skipped: 0, skippedLines: [] };
}

function recordSkip(stats: ParseStats | undefined, lineNumber: number): void {
  if (!stats) return;
  stats.skipped += 1;
  if (stats.skippedLines.length < 5) stats.skippedLines.push(lineNumber);
}

function splitLine(line: string, delimiter: string): string[] {
  // Minimal CSV split — handles a quoted field containing the delimiter,
  // which is the one real-world wrinkle in CMS/CDC's published code-set
  // files (descriptions sometimes contain commas). Not a full RFC 4180
  // parser — swap in a dedicated CSV library here if a real source needs
  // more (escaped quotes, embedded newlines, etc.).
  const fields: string[] = [];
  let current = '';
  let inQuotes = false;

  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (char === '"') {
      inQuotes = !inQuotes;
    } else if (char === delimiter && !inQuotes) {
      fields.push(current.trim());
      current = '';
    } else {
      current += char;
    }
  }
  fields.push(current.trim());

  return fields.map((field) => field.replace(/^"|"$/g, ''));
}

/**
 * Parses a delimited text stream into {@link CodeSetRow}s. This is the
 * reference row source for the free code sets (ICD-10-CM, HCPCS) — the
 * exact real file layout CMS/CDC publish varies by release and isn't
 * pinned here; adjust {@link CsvColumnMap} to match whatever the actual
 * downloaded file looks like. See plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §7.
 */
export async function* parseCsvRows(
  source: Readable,
  columnMap: CsvColumnMap,
  stats?: ParseStats
): AsyncIterable<CodeSetRow> {
  for (const [name, value] of Object.entries({
    code: columnMap.code,
    description: columnMap.description,
    effectiveDate: columnMap.effectiveDate
  })) {
    if (value !== undefined && (!Number.isInteger(value) || value < 0)) {
      throw new Error(`Column map: ${name} must be a column index (0 or more), got ${value}`);
    }
  }
  const delimiter = columnMap.delimiter ?? ',';
  const hasHeader = columnMap.hasHeader ?? true;
  const rl = createInterface({ input: source, crlfDelay: Infinity });

  let lineNumber = 0;
  for await (const line of rl) {
    lineNumber += 1;
    if (line.trim().length === 0) continue;
    if (hasHeader && lineNumber === 1) continue;

    const fields = splitLine(line, delimiter);
    const code = fields[columnMap.code]?.trim();
    const description = fields[columnMap.description]?.trim();
    if (!code || !description) {
      recordSkip(stats, lineNumber);
      continue;
    }

    const row: CodeSetRow = { code, description };
    if (columnMap.effectiveDate != null) {
      const raw = fields[columnMap.effectiveDate]?.trim();
      if (raw) {
        const parsed = new Date(raw);
        if (!Number.isNaN(parsed.getTime())) {
          row.effectiveDate = parsed;
        }
      }
    }

    yield row;
  }
}

/**
 * Parses the CDC/NCHS ICD-10-CM codes file (icd10cm_codes_YYYY.txt): one
 * code per line, the code without its dot, then spaces, then the
 * description, and no header row:
 *
 *   A000    Cholera due to Vibrio cholerae 01, biovar cholerae
 *
 * A comma-delimited parse of this file reads the whole line as the code.
 */
export async function* parseCodeThenDescriptionLines(
  source: Readable,
  stats?: ParseStats
): AsyncIterable<CodeSetRow> {
  const rl = createInterface({ input: source, crlfDelay: Infinity });
  let lineNumber = 0;
  for await (const raw of rl) {
    lineNumber += 1;
    const line = raw.trim();
    if (line.length === 0) continue;
    const gap = line.search(/\s/);
    const code = gap > 0 ? line.slice(0, gap) : '';
    const description = gap > 0 ? line.slice(gap).trim() : '';
    if (!code || !description) {
      recordSkip(stats, lineNumber);
      continue;
    }
    yield { code, description };
  }
}
