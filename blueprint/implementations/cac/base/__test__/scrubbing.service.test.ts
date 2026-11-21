import { ScrubbingService } from '../services/scrubbing.service';

const scrubbing = new ScrubbingService();

describe('ScrubbingService', () => {
  it('returns clean for a single line justified by its diagnosis', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-002', units: 1 }],
      ['Z00.00']
    );

    expect(result.clean).toBe(true);
    expect(result.findings).toEqual([]);
  });

  it('flags an NCCI PTP conflict between two procedures billed together', () => {
    const result = scrubbing.scrub(
      [
        { procedureCode: 'PROC-001', units: 1 },
        { procedureCode: 'PROC-002', units: 1 }
      ],
      ['J06.9', 'Z00.00']
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'ncci_ptp', carcCode: 'CO-97' })
    );
  });

  it('does not flag a PTP conflict for procedures billed alone', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-001', units: 1 }],
      ['J06.9']
    );

    expect(result.findings.some((f) => f.category === 'ncci_ptp')).toBe(
      false
    );
  });

  it('flags an NCCI MUE violation when units exceed the mock cap', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-001', units: 2 }],
      ['J06.9']
    );

    expect(result.clean).toBe(false);
    // CARC 151, the real code for unit counts the payer won't support
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'ncci_mue', carcCode: 'CO-151' })
    );
  });

  it('allows units within the mock cap', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-003', units: 3 }],
      ['R73.09']
    );

    expect(result.findings.some((f) => f.category === 'ncci_mue')).toBe(
      false
    );
  });

  it('flags an LCD/NCD medical-necessity violation for an unjustified diagnosis', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-002', units: 1 }],
      ['J06.9']
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'lcd_ncd', carcCode: 'CO-50' })
    );
  });

  it('does not flag LCD/NCD for a procedure with no mock crosswalk entry', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-999', units: 1 }],
      ['J06.9']
    );

    expect(result.findings.some((f) => f.category === 'lcd_ncd')).toBe(
      false
    );
  });

  it('flags required_fields when a claim has no diagnosis codes at all', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-999', units: 1 }], // no LCD/NCD crosswalk entry
      []
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Claim has no diagnosis codes'
      })
    );
  });

  it('flags required_fields when a charge line is missing a procedure code', () => {
    const result = scrubbing.scrub([{ procedureCode: '', units: 1 }], ['J06.9']);

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Charge line 1 is missing a procedure code'
      })
    );
  });

  it('flags required_fields for a non-positive unit count', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-001', units: 0 }],
      ['J06.9']
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'required_fields', carcCode: 'CO-16' })
    );
  });

  it('does not flag required_fields for a complete, valid claim', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-001', units: 1 }],
      ['J06.9']
    );

    expect(
      result.findings.some((f) => f.category === 'required_fields')
    ).toBe(false);
  });

  it('can return multiple findings for a single line at once', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: 'PROC-001', units: 5 }],
      ['Z00.00']
    );

    expect(result.findings.some((f) => f.category === 'ncci_mue')).toBe(
      true
    );
    expect(result.findings.some((f) => f.category === 'lcd_ncd')).toBe(
      true
    );
  });

  it('flags required_fields when a claim has no charge lines', () => {
    const result = scrubbing.scrub([], ['J06.9']);

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Claim has no charge lines'
      })
    );
  });

  it('treats blank diagnosis codes as missing', () => {
    const onlyBlank = scrubbing.scrub(
      [{ procedureCode: 'PROC-999', units: 1 }],
      [' ']
    );
    expect(onlyBlank.clean).toBe(false);
    expect(onlyBlank.findings).toEqual([
      {
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Diagnosis code 1 is blank'
      }
    ]);

    const oneBlank = scrubbing.scrub(
      [{ procedureCode: 'PROC-002', units: 1 }],
      ['Z00.00', '']
    );
    expect(oneBlank.findings).toContainEqual(
      expect.objectContaining({
        category: 'required_fields',
        message: 'Diagnosis code 2 is blank'
      })
    );
  });

  it('applies the MUE cap to a code summed over its lines', () => {
    const result = scrubbing.scrub(
      [
        { procedureCode: 'PROC-004', units: 1 },
        { procedureCode: 'PROC-004', units: 1 }
      ],
      ['J06.9']
    );

    expect(result.findings).toContainEqual(
      expect.objectContaining({
        category: 'ncci_mue',
        message: 'PROC-004 billed at 2 units, exceeds the mock unit cap of 1'
      })
    );
  });

  it('matches rule tables however a code is written', () => {
    // the crosswalk justifies PROC-005 with E11.9; E119 is the CDC form
    for (const diagnosis of ['E11.9', 'E119', 'e11.9', ' E119 ']) {
      expect([diagnosis, scrubbing.scrub([{ procedureCode: 'PROC-005', units: 1 }], [diagnosis])])
        .toEqual([diagnosis, { clean: true, findings: [] }]);
    }
    // the same procedure written two ways is one MUE bucket (cap 1)
    const mue = scrubbing.scrub(
      [
        { procedureCode: 'PROC-005', units: 1 },
        { procedureCode: 'proc-005', units: 1 }
      ],
      ['E119']
    );
    expect(mue.findings.map((f) => f.category)).toEqual(['ncci_mue']);
    // PTP pairs match whatever the case
    const ptp = scrubbing.scrub(
      [
        { procedureCode: 'proc-001', units: 1 },
        { procedureCode: 'PROC-002', units: 1 }
      ],
      ['J06.9', 'Z00.00']
    );
    expect(ptp.findings.map((f) => f.category)).toEqual(['ncci_ptp']);
  });

  it('does not take inherited object keys for rule entries', () => {
    const result = scrubbing.scrub(
      [
        { procedureCode: 'toString', units: 1 },
        { procedureCode: 'constructor', units: 5 }
      ],
      ['J06.9']
    );

    expect(
      result.findings.filter(
        (f) => f.category === 'lcd_ncd' || f.category === 'ncci_mue'
      )
    ).toEqual([]);
  });

  it('reports each blank field once', () => {
    const result = scrubbing.scrub(
      [
        { procedureCode: 'PROC-999', units: 1 },
        { procedureCode: ' ', units: 1 }
      ],
      ['', 'J06.9', ' ']
    );

    expect(result.findings).toEqual([
      {
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Diagnosis code 1 is blank'
      },
      {
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Diagnosis code 3 is blank'
      },
      {
        category: 'required_fields',
        carcCode: 'CO-16',
        message: 'Charge line 2 is missing a procedure code',
        chargeLine: 2
      }
    ]);
  });

  it('says which charge line and code each finding is about', () => {
    const result = scrubbing.scrub(
      [
        { procedureCode: 'PROC-001', units: 1 },
        { procedureCode: 'PROC-002', units: 0 },
        { procedureCode: 'PROC-004', units: 2 }
      ],
      ['J06.9']
    );

    expect(result.findings).toEqual([
      {
        category: 'required_fields',
        carcCode: 'CO-16',
        message:
          'Charge line 2 has an invalid unit count (0); must be a positive integer',
        chargeLine: 2,
        code: 'PROC-002'
      },
      {
        category: 'ncci_ptp',
        carcCode: 'CO-97',
        message:
          'Charge lines 1 and 2: PROC-001 and PROC-002 cannot be billed together without a justifying modifier',
        chargeLine: 2,
        code: 'PROC-002'
      },
      {
        category: 'ncci_mue',
        carcCode: 'CO-151',
        message: 'PROC-004 billed at 2 units, exceeds the mock unit cap of 1',
        code: 'PROC-004'
      },
      {
        category: 'lcd_ncd',
        carcCode: 'CO-50',
        message:
          'Charge line 2: no diagnosis on this claim justifies PROC-002 as medically necessary',
        chargeLine: 2,
        code: 'PROC-002'
      },
      {
        category: 'lcd_ncd',
        carcCode: 'CO-50',
        message:
          'Charge line 3: no diagnosis on this claim justifies PROC-004 as medically necessary',
        chargeLine: 3,
        code: 'PROC-004'
      }
    ]);
  });
});
