export const SmsStatusEnum = {
  QUEUED: 'queued',
  SENT: 'sent',
  DELIVERED: 'delivered',
  FAILED: 'failed',
  UNDELIVERED: 'undelivered'
} as const;
export type SmsStatusEnum = (typeof SmsStatusEnum)[keyof typeof SmsStatusEnum];
