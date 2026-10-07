import { isTrue } from '@forklaunch/common';
import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';
import {
  SendSmsDto,
  SmsRecordDto
} from '@forklaunch/interfaces-messaging/types';
import { testSchemaEquality } from '@forklaunch/internal';
import {
  SendSmsSchema as TypeboxSendSmsSchema,
  SmsRecordSchema as TypeboxSmsRecordSchema
} from '../domain/schemas/typebox/sms.schema';
import {
  SendSmsSchema as ZodSendSmsSchema,
  SmsRecordSchema as ZodSmsRecordSchema
} from '../domain/schemas/zod/sms.schema';

const zodSmsRecordSchema = ZodSmsRecordSchema({ uuidId: false });
const typeboxSmsRecordSchema = TypeboxSmsRecordSchema({ uuidId: false });

describe('schema equality', () => {
  it('should be equal for send sms', () => {
    expect(
      isTrue(
        testSchemaEquality<SendSmsDto>()(ZodSendSmsSchema, TypeboxSendSmsSchema, {
          to: '+15555550100',
          body: 'test message',
          metadata: {
            test: 'test'
          }
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for sms record', () => {
    expect(
      isTrue(
        testSchemaEquality<SmsRecordDto<typeof SmsStatusEnum>>()(
          zodSmsRecordSchema,
          typeboxSmsRecordSchema,
          {
            id: 'test',
            to: '+15555550100',
            body: 'test message',
            status: SmsStatusEnum.DELIVERED,
            providerMessageId: 'SM123',
            error: 'test error',
            metadata: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();
  });
});
