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

// RECID values in the CMS HCPCS Level II release: 3 starts a procedure
// code's record and 4 continues its long description; 7 and 8 are the
// same for modifiers, which are not procedure codes and are not loaded.
const HCPCS_RECID_PROCEDURE = '3';
const HCPCS_RECID_PROCEDURE_CONTINUED = '4';
const HCPCS_RECID_MODIFIER = new Set(['7', '8']);

/**
 * Parses a CSV export of the CMS HCPCS Level II release
 * (HCPC2026_JAN_ANWEB and similar). Columns are found by header name, so
 * extra or reordered columns are fine:
 *
 *   HCPC,SEQNUM,RECID,LONG DESCRIPTION,SHORT DESCRIPTION,...
 *   A0021,00100,3,"Ambulance service, outside state per mile, transport",...
 *   A0021,00200,4,"(Medicaid only)",...
 *
 * A long description longer than one record continues on RECID 4 rows;
 * those are joined onto the code's description. Modifier rows (RECID 7
 * and 8) are left out. A row with any other RECID, or without a code or
 * description, counts as skipped.
 *
 * Without HCPC and RECID headers, the file is read as a plain
 * code,description CSV with a header row.
 */
export async function* parseHcpcsReleaseRows(
  source: Readable,
  stats?: ParseStats
): AsyncIterable<CodeSetRow> {
  const rl = createInterface({ input: source, crlfDelay: Infinity });
  let lineNumber = 0;
  let layout:
    | { kind: 'cms'; code: number; recid: number; description: number }
    | { kind: 'plain' }
    | undefined;
  let pending: CodeSetRow | undefined;

  for await (const line of rl) {
    lineNumber += 1;
    if (line.trim().length === 0) continue;
    const fields = splitLine(line, ',');

    if (!layout) {
      const headers = fields.map((h) => h.trim().toUpperCase());
      const code = headers.indexOf('HCPC');
      const recid = headers.indexOf('RECID');
      const description = headers.indexOf('LONG DESCRIPTION');
      layout =
        code >= 0 && recid >= 0 && description >= 0
          ? { kind: 'cms', code, recid, description }
          : { kind: 'plain' };
      continue;
    }

    if (layout.kind === 'plain') {
      const code = fields[0]?.trim();
      const description = fields[1]?.trim();
      if (!code || !description) {
        recordSkip(stats, lineNumber);
        continue;
      }
      yield { code, description };
      continue;
    }

    const code = fields[layout.code]?.trim();
    const recid = fields[layout.recid]?.trim();
    const description = fields[layout.description]?.trim();
    if (recid && HCPCS_RECID_MODIFIER.has(recid)) continue;
    if (recid === HCPCS_RECID_PROCEDURE_CONTINUED && pending && code === pending.code) {
      if (description) pending.description += ` ${description}`;
      continue;
    }
    if (recid !== HCPCS_RECID_PROCEDURE || !code || !description) {
      recordSkip(stats, lineNumber);
      continue;
    }
    if (pending) yield pending;
    pending = { code, description };
  }
  if (pending) yield pending;
}
