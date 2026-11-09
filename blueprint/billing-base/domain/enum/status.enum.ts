export const StatusEnum = {
  PENDING: 'PENDING',
  SUCCESS: 'SUCCESS',
  CREATED: 'CREATED',
  UPDATED: 'UPDATED',
  COMPLETED: 'COMPLETED',
  CANCELLED: 'CANCELLED',
  FAILED: 'FAILED',
  EXPIRED: 'EXPIRED'
} as const;
export type StatusEnum = (typeof StatusEnum)[keyof typeof StatusEnum];
