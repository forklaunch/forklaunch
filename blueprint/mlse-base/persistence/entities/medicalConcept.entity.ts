import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * A MeSH descriptor (public domain, NLM): the controlled vocabulary MLSE uses
 * to treat different names for the same thing as one concept, e.g.
 * "Laparoscopic Cholecystectomy" and "Cholecystectomy, Laparoscopic".
 */
export const MedicalConcept = defineComplianceEntity({
  name: 'MedicalConcept',
  properties: {
    ...sqlBaseProperties,
    descriptorUi: fp.string().unique().compliance('none'),
    preferredTerm: fp.string().compliance('none'),
    synonyms: fp.string().array().compliance('none'),
    treeNumbers: fp.string().array().compliance('none')
  }
});

export type MedicalConcept = InferEntity<typeof MedicalConcept>;
