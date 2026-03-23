{{#is_worker}}import type { WorkerEventEntity } from '@forklaunch/interfaces-worker/types';

export interface {{pascal_case_name}}EventRecord extends WorkerEventEntity {
  message: string;
  createdAt: Date;
  updatedAt: Date;
}
{{/is_worker}}