import { Readable } from 'node:stream';
import { normalizeCode } from '../domain/codeNormalization';
import {
  newParseStats,
  parseCodeThenDescriptionLines,
  parseCsvRows
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
