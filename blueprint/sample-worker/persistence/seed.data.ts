import { type InferEntity, type RequiredEntityData } from '@mikro-orm/core';
import { SampleWorkerEventRecord as SampleWorkerEventRecordEntity } from './entities/sampleWorkerRecord.entity';
//! Begin seed data
export const sampleWorkerEventRecord = {
  message: 'Hello, world!',
  processed: false,
  retryCount: 0
} satisfies RequiredEntityData<
  InferEntity<typeof SampleWorkerEventRecordEntity>
>;
