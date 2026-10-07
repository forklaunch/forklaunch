// Delivery statuses reported by the Twilio Messages API and its status
// callbacks: https://www.twilio.com/docs/messaging/api/message-resource
export const TwilioMessageStatusEnum = {
  QUEUED: 'queued',
  ACCEPTED: 'accepted',
  SCHEDULED: 'scheduled',
  CANCELED: 'canceled',
  SENDING: 'sending',
  SENT: 'sent',
  DELIVERED: 'delivered',
  UNDELIVERED: 'undelivered',
  FAILED: 'failed',
  RECEIVING: 'receiving',
  RECEIVED: 'received',
  READ: 'read'
} as const;
export type TwilioMessageStatusEnum =
  (typeof TwilioMessageStatusEnum)[keyof typeof TwilioMessageStatusEnum];
