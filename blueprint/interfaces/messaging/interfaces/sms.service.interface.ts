import { SmsServiceParameters } from '../types/sms.service.types';

export interface SmsService<
  StatusEnum,
  Params extends
    SmsServiceParameters<StatusEnum> = SmsServiceParameters<StatusEnum>
> {
  // dispatches an sms message via the configured provider and records it
  sendSms: (smsDto: Params['SendSmsDto']) => Promise<Params['SmsRecordDto']>;
  getSmsRecord: (idDto: Params['IdDto']) => Promise<Params['SmsRecordDto']>;
}
