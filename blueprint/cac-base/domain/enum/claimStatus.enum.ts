export const ClaimStatus = {
  DRAFT: 'draft',
  SCRUBBING: 'scrubbing',
  READY: 'ready',
  SUBMITTED: 'submitted',
  ACCEPTED: 'accepted',
  DENIED: 'denied',
  PAID: 'paid',
  VOID: 'void'
} as const;
export type ClaimStatus = (typeof ClaimStatus)[keyof typeof ClaimStatus];
