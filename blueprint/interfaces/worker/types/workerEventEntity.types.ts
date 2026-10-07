export type WorkerEventEntity = {
  id: string;
  retryCount: number;
  processed: boolean;
  createdAt: Date;
  updatedAt: Date;
};
