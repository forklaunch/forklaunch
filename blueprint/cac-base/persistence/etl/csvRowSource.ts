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
  columnMap: CsvColumnMap
): AsyncIterable<CodeSetRow> {
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
    if (!code || !description) continue;

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
