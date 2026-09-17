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
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'ncci_mue' })
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
});
