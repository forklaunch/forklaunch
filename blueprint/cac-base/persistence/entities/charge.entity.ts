import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Encounter } from './encounter.entity';

export const Charge = defineComplianceEntity({
  name: 'Charge',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    encounter: () => fp.manyToOne(Encounter),
    // Either a MockProcedureCodeProvider code or, once an org's own real
    // connector is active (§5), a real CPT code supplied by that org — the
    // column shape is identical either way, 'none' since it's just a code.
    procedureCode: fp.string().compliance('none'),
    // Matters for NCCI MUE unit-cap checks in the scrubbing engine (§6).
    units: fp.integer().compliance('none'),
    amount: fp.double().compliance('none')
  }
});
