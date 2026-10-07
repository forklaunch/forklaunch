import { IdDto, RecordTimingDto } from '@forklaunch/common';

export type SendSmsDto = {
  // E.164 formatted destination phone number
  to: string;
  body: string;
  metadata?: Record<string, unknown>;
};

export type SmsRecordDto<StatusEnum> = SendSmsDto &
  IdDto & {
    status: StatusEnum[keyof StatusEnum];
    providerMessageId?: string;
    error?: string;
  } & Partial<RecordTimingDto>;

export type SmsServiceParameters<StatusEnum> = {
  SendSmsDto: SendSmsDto;
  SmsRecordDto: SmsRecordDto<StatusEnum>;
  IdDto: IdDto;
};
