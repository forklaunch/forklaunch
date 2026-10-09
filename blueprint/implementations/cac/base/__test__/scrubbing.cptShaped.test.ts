import { ScrubbingService } from '../services/scrubbing.service';

// Proves the scrubbing engine (§6) has no hidden assumption that only works
// against MockProcedureCodeProvider's "PROC-XXX" placeholder format — the
// exact same engine, unmodified, correctly handles CPT-*shaped* codes (real
// numeric structure, no real AMA content). This is §5's readiness-bar item
// 4: proof the engine survives contact with real-shaped data before any
// organization ever wires in an actual licensed CPT feed.
const scrubbing = new ScrubbingService();

describe('ScrubbingService against the CPT-shaped fixture', () => {
  it('flags an NCCI PTP conflict between two synthetic Category I codes', () => {
    const result = scrubbing.scrub(
      [
        { procedureCode: '10001', units: 1 },
        { procedureCode: '10002', units: 1 }
      ],
      ['Z00.00']
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'ncci_ptp', carcCode: 'CO-97' })
    );
  });

  it('flags an NCCI MUE violation for a synthetic Category III code', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: '0001T', units: 3 }],
      ['Z00.00']
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'ncci_mue' })
    );
  });

  it('flags an LCD/NCD violation for a synthetic Category I code with no justifying diagnosis', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: '90001', units: 1 }],
      ['J06.9']
    );

    expect(result.clean).toBe(false);
    expect(result.findings).toContainEqual(
      expect.objectContaining({ category: 'lcd_ncd', carcCode: 'CO-50' })
    );
  });

  it('passes a CPT-shaped claim that is clean on all three layers', () => {
    const result = scrubbing.scrub(
      [{ procedureCode: '90001', units: 1 }],
      ['Z00.00']
    );

    expect(result.clean).toBe(true);
  });
});
