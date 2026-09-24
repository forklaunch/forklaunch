// Status of a Denial row on the denial worklist — see services/denialWorklist.service.ts.
export const WorklistStatus = {
  OPEN: 'open',
  RESOLVED: 'resolved'
} as const;
export type WorklistStatus =
  (typeof WorklistStatus)[keyof typeof WorklistStatus];
