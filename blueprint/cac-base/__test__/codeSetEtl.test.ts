import { Readable } from 'node:stream';
import { normalizeCode } from '../domain/codeNormalization';
import {
  newParseStats,
  parseCodeThenDescriptionLines,
  parseCsvRows,
  parseHcpcsReleaseRows
} from '../persistence/etl/csvRowSource';
import { columnIndexFromEnv, flagFromEnv } from '../persistence/etl/env';

async function collect<T>(rows: AsyncIterable<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const row of rows) out.push(row);
  return out;
}

describe('normalizeCode', () => {
  it('compares codes without dot, spaces or case', () => {
    for (const written of ['E11.9', 'E119', 'e11.9', ' E119 ', 'E 11.9']) {
      expect(normalizeCode(written)).toBe('E119');
    }
    expect(normalizeCode('j3490')).toBe('J3490');
  });
});

describe('parseCodeThenDescriptionLines (CDC ICD-10-CM codes file)', () => {
  it('reads the code and the whole description, with no header row', async () => {
    const file = [
      'A000    Cholera due to Vibrio cholerae 01, biovar cholerae',
      'A001    Cholera due to Vibrio cholerae 01, biovar eltor',
      '',
      'E119    Type 2 diabetes mellitus without complications'
    ].join('\r\n');
    const rows = await collect(parseCodeThenDescriptionLines(Readable.from([file])));
    expect(rows).toEqual([
      { code: 'A000', description: 'Cholera due to Vibrio cholerae 01, biovar cholerae' },
      { code: 'A001', description: 'Cholera due to Vibrio cholerae 01, biovar eltor' },
      { code: 'E119', description: 'Type 2 diabetes mellitus without complications' }
    ]);
  });

  it('counts a line with no description as skipped', async () => {
    const stats = newParseStats();
    const rows = await collect(
      parseCodeThenDescriptionLines(Readable.from(['A000    Cholera\nA001\n']), stats)
    );
    expect(rows).toHaveLength(1);
    expect(stats).toEqual({ skipped: 1, skippedLines: [2] });
  });
});

describe('parseCsvRows', () => {
  it('counts lines without the mapped columns as skipped', async () => {
    const stats = newParseStats();
    const rows = await collect(
      parseCsvRows(
        Readable.from(['code,description\nA00,Cholera\nA01 Typhoid fever\n']),
        { code: 0, description: 1 },
        stats
      )
    );
    expect(rows).toEqual([{ code: 'A00', description: 'Cholera' }]);
    expect(stats.skipped).toBe(1);
  });

  it('refuses a column map that is not a column index', async () => {
    await expect(
      collect(parseCsvRows(Readable.from(['A00,Cholera']), { code: Number('abc'), description: 1 }))
    ).rejects.toThrow(/code must be a column index/);
  });
});

describe('parseHcpcsReleaseRows (CMS HCPCS Level II release)', () => {
  it('reads procedure records, joins continuation lines and leaves out modifiers', async () => {
    const file = [
      'HCPC,SEQNUM,RECID,LONG DESCRIPTION,SHORT DESCRIPTION',
      'A0021,00100,3,"Ambulance service, outside state per mile, transport","Outside state ambulance serv"',
      'A0021,00200,4,"(Medicaid only)",',
      'J3490,00100,3,"Unclassified drugs","Drugs unclassified injection"',
      'AA,00100,7,"Anesthesia services performed personally by anesthesiologist","Anes perf by anesthesiologist"',
      'AA,00200,8,"(continued modifier text)",'
    ].join('\r\n');
    const stats = newParseStats();
    const rows = await collect(parseHcpcsReleaseRows(Readable.from([file]), stats));
    expect(rows).toEqual([
      {
        code: 'A0021',
        description: 'Ambulance service, outside state per mile, transport (Medicaid only)'
      },
      { code: 'J3490', description: 'Unclassified drugs' }
    ]);
    expect(stats.skipped).toBe(0);
  });

  it('finds the columns by header name, in any order', async () => {
    const file = ['RECID,LONG DESCRIPTION,HCPC', '3,"Unclassified drugs",J3490'].join('\n');
    const rows = await collect(parseHcpcsReleaseRows(Readable.from([file])));
    expect(rows).toEqual([{ code: 'J3490', description: 'Unclassified drugs' }]);
  });

  it('counts a record it does not recognize as skipped', async () => {
    const file = [
      'HCPC,SEQNUM,RECID,LONG DESCRIPTION',
      'J3490,00100,3,"Unclassified drugs"',
      'G0438,00100,9,"Annual wellness visit"',
      'G0439,00100,3,'
    ].join('\n');
    const stats = newParseStats();
    const rows = await collect(parseHcpcsReleaseRows(Readable.from([file]), stats));
    expect(rows.map((r) => r.code)).toEqual(['J3490']);
    expect(stats.skipped).toBe(2);
  });

  it('reads a plain code,description CSV when the CMS headers are absent', async () => {
    const file = ['code,description', 'J3490,"Unclassified drugs"'].join('\n');
    const rows = await collect(parseHcpcsReleaseRows(Readable.from([file])));
    expect(rows).toEqual([{ code: 'J3490', description: 'Unclassified drugs' }]);
  });

  it('reads a plain code,description CSV with no header row, first line included', async () => {
    const file = ['J3490,"Unclassified drugs"', 'G0438,"Annual wellness visit, initial"'].join('\n');
    const stats = newParseStats();
    const rows = await collect(parseHcpcsReleaseRows(Readable.from([file]), stats));
    expect(rows).toEqual([
      { code: 'J3490', description: 'Unclassified drugs' },
      { code: 'G0438', description: 'Annual wellness visit, initial' }
    ]);
    expect(stats.skipped).toBe(0);
  });

  it('reads the CMS headers behind a byte order mark', async () => {
    const file = ['\uFEFFHCPC,SEQNUM,RECID,LONG DESCRIPTION', 'J3490,00100,3,"Unclassified drugs"'].join(
      '\n'
    );
    const rows = await collect(parseHcpcsReleaseRows(Readable.from([file])));
    expect(rows).toEqual([{ code: 'J3490', description: 'Unclassified drugs' }]);
  });

  // Each of these used to be read as the plain layout: the sequence number
  // (column 2) was loaded as every code's description and the load
  // reported success.
  it.each([
    ['a renamed description column', 'HCPC,SEQNUM,RECID,LONG_DESCRIPTION,SHORT DESCRIPTION'],
    ['a renamed code column', 'HCPCS,SEQNUM,RECID,LONG DESCRIPTION'],
    ['a missing RECID column', 'HCPC,SEQNUM,LONG DESCRIPTION'],
    ['headers it does not know at all', 'id,name,notes']
  ])('refuses a header row with %s', async (_label, header) => {
    const file = [header, 'J3490,00100,3,"Unclassified drugs","Drugs unclassified injection"'].join('\n');
    const stats = newParseStats();
    const rows: unknown[] = [];
    await expect(
      (async () => {
        for await (const row of parseHcpcsReleaseRows(Readable.from([file]), stats)) rows.push(row);
      })()
    ).rejects.toThrow(/HCPCS file: the first line .* is not a header row this loader recognises/);
    expect(rows).toEqual([]);
  });

  it('refuses a CMS-layout file with no header row instead of guessing the columns', async () => {
    const file = ['J3490,00100,3,"Unclassified drugs"', 'G0438,00100,3,"Annual wellness visit"'].join('\n');
    await expect(collect(parseHcpcsReleaseRows(Readable.from([file])))).rejects.toThrow(
      /not a header row this loader recognises/
    );
  });
});

describe('environment settings for the refresh script', () => {
  it('reads a column index, or refuses anything else', () => {
    expect(columnIndexFromEnv('CPT_CODE_COLUMN', undefined, 0)).toBe(0);
    expect(columnIndexFromEnv('CPT_CODE_COLUMN', '2', 0)).toBe(2);
    expect(() => columnIndexFromEnv('CPT_CODE_COLUMN', 'abc', 0)).toThrow(/CPT_CODE_COLUMN/);
    expect(() => columnIndexFromEnv('CPT_CODE_COLUMN', '-1', 0)).toThrow();
    expect(() => columnIndexFromEnv('CPT_CODE_COLUMN', '1.5', 0)).toThrow();
  });

  it('reads true/false flags, or refuses anything else', () => {
    expect(flagFromEnv('CPT_HAS_HEADER', undefined, true)).toBe(true);
    expect(flagFromEnv('CPT_HAS_HEADER', 'FALSE', true)).toBe(false);
    expect(() => flagFromEnv('CPT_HAS_HEADER', 'no', true)).toThrow(/CPT_HAS_HEADER/);
  });
});
