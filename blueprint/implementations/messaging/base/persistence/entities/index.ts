import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const SmsRecord = defineComplianceEntity({
  name: 'SmsRecord',
  properties: {
    id: fp.string().primary().compliance('none'),
    to: fp.string().compliance('pii'),
    body: fp.string().compliance('pii'),
    status: fp.enum<string[]>().compliance('none'),
    providerMessageId: fp.string().nullable().compliance('none'),
    error: fp.string().nullable().compliance('none'),
    metadata: fp.json<unknown>().nullable().compliance('none')
  }
});
