import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

// Entity that defines the structure of the SampleWorkerEventRecord table
export const SampleWorkerEventRecord = defineComplianceEntity({
  name: 'SampleWorkerEventRecord',
  properties: {
    ...sqlBaseProperties,
    message: fp.string().compliance('none'),
    processed: fp.boolean().compliance('none'),
    retryCount: fp.integer().compliance('none')
  }
});

export type SampleWorkerEventRecord = InferEntity<
  typeof SampleWorkerEventRecord
>;
