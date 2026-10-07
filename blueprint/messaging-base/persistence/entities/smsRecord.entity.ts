import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';

// This represents an sms message dispatched through the messaging service
export const SmsRecord = defineComplianceEntity({
  name: 'SmsRecord',
  properties: {
    ...sqlBaseProperties,
    to: fp.string().compliance('pii'),
    body: fp.string().compliance('pii'),
    status: fp.enum(() => SmsStatusEnum).compliance('none'),
    providerMessageId: fp.string().nullable().compliance('none'),
    error: fp.string().nullable().compliance('none'),
    metadata: fp.json<unknown>().nullable().compliance('none')
  }
});
