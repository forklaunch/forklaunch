import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Encounter } from './encounter.entity';

export const Charge = defineComplianceEntity({
  name: 'Charge',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    encounter: () => fp.manyToOne(Encounter).deleteRule('cascade').updateRule('cascade'),
    // Either a MockProcedureCodeProvider code or, once an org's own real
    // connector is active (§5), a real CPT code supplied by that org — the
    // column shape is identical either way, 'none' since it's just a code.
    procedureCode: fp.string().compliance('none'),
    // Matters for NCCI MUE unit-cap checks in the scrubbing engine (§6).
    units: fp.integer().compliance('none'),
    // numeric(12,2), not double precision: this is currency on a billable
    // claim line. Binary floating point can't represent most cent values
    // exactly, so sums drift and reconciliation against an 835 fails by
    // fractions of a cent. 'number' keeps the runtime type a JS number
    // (nothing aggregates this column yet); the exactness that matters is
    // in the column.
    amount: fp.decimal('number').precision(12).scale(2).compliance('none'),
    // The line's position as the coder entered it (1, 2, 3...). Findings
    // number lines in this order, so "line 2" is the second line entered.
    // Optional: rows without one fall back to createdAt, then id, which is
    // stable but cannot tell apart rows saved in the same millisecond.
    lineNumber: fp.integer().nullable().compliance('none')
  }
});
