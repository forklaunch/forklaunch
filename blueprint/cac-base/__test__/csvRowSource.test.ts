import { Readable } from 'node:stream';
import { parseCsvRows } from '../persistence/etl/csvRowSource';

async function collect(stream: Readable, columnMap: Parameters<typeof parseCsvRows>[1]) {
  const rows = [];
  for await (const row of parseCsvRows(stream, columnMap)) {
    rows.push(row);
  }
  return rows;
}

describe('parseCsvRows', () => {
  it('parses code/description rows and skips the header', async () => {
    const csv = ['code,description', 'A00,Cholera', 'A01,Typhoid fever'].join(
      '\n'
    );
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1
    });

    expect(rows).toEqual([
      { code: 'A00', description: 'Cholera' },
      { code: 'A01', description: 'Typhoid fever' }
    ]);
  });

  it('keeps quotes that a CSV export writes doubled inside a quoted field', async () => {
    // As the CMS HCPCS release comes out of a spreadsheet's CSV export:
    // a quoted word and an inch mark, both doubled.
    const csv = [
      'code,description',
      'A4719,"""y set"" tubing for peritoneal dialysis"',
      'E1031,"Rollabout chair, any and all types with casters 5"" or greater"'
    ].join('\n');
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1
    });

    expect(rows).toEqual([
      { code: 'A4719', description: '"y set" tubing for peritoneal dialysis' },
      {
        code: 'E1031',
        description: 'Rollabout chair, any and all types with casters 5" or greater'
      }
    ]);
  });

  it('keeps a quote inside an unquoted field as written', async () => {
    const csv = ['code,description', 'E1031,Casters 5" or greater'].join('\n');
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1
    });

    expect(rows).toEqual([
      { code: 'E1031', description: 'Casters 5" or greater' }
    ]);
  });

  it('handles a quoted field containing the delimiter', async () => {
    const csv = [
      'code,description',
      'A02,"Other salmonella, infections"'
    ].join('\n');
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1
    });

    expect(rows).toEqual([
      { code: 'A02', description: 'Other salmonella, infections' }
    ]);
  });

  it('parses an optional effective-date column', async () => {
    const csv = ['code,description,effective', 'A00,Cholera,2025-10-01'].join(
      '\n'
    );
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1,
      effectiveDate: 2
    });

    expect(rows).toHaveLength(1);
    expect(rows[0].code).toBe('A00');
    expect(rows[0].effectiveDate).toBeInstanceOf(Date);
  });

  it('skips blank lines and rows missing a code or description', async () => {
    const csv = ['code,description', '', 'A00,Cholera', 'A01,'].join('\n');
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1
    });

    expect(rows).toEqual([{ code: 'A00', description: 'Cholera' }]);
  });

  it('respects hasHeader: false', async () => {
    const csv = ['A00,Cholera'].join('\n');
    const rows = await collect(Readable.from([csv]), {
      code: 0,
      description: 1,
      hasHeader: false
    });

    expect(rows).toEqual([{ code: 'A00', description: 'Cholera' }]);
  });
});
