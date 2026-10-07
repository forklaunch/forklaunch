import {
  SendSmsDto,
  SmsRecordDto
} from '@forklaunch/interfaces-messaging/types';

// sms dto types
export type BaseSmsDtos<StatusEnum> = {
  SmsRecordMapper: SmsRecordDto<StatusEnum>;
  SendSmsMapper: SendSmsDto;
};
