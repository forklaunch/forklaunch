export type WorkerEventEntity = {
  id: string;
  tenantId: string;
  retryCount: number;
  processed: boolean;
  createdAt: Date;
  updatedAt: Date;
};
