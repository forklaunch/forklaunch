import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

// Entity that defines the structure of the SampleWorkerEventRecord table
export const SampleWorkerEventRecord = defineEntity({
  name: 'SampleWorkerEventRecord',
  properties: {
    ...sqlBaseProperties,
    message: p.string(),
    processed: p.boolean(),
    retryCount: p.integer()
  }
});

export type SampleWorkerEventRecord = InferEntity<
  typeof SampleWorkerEventRecord
>;
