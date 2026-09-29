export const DocumentStatus = {
  CURRENT: 'current',
  SUPERSEDED: 'superseded',
  RETRACTED: 'retracted'
} as const;
export type DocumentStatus = (typeof DocumentStatus)[keyof typeof DocumentStatus];
