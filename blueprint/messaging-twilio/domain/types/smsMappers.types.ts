import { SchemaValidator } from '@forklaunch/blueprint-core';
import { Schema } from '@forklaunch/validator';
import { SmsRecord } from '../../persistence/entities';
import {
  SendSmsMapper,
  SmsRecordMapper
} from '../mappers/smsRecord.mappers';

// sms record mappers
export type SmsMapperTypes = {
  SmsRecordMapper: typeof SmsRecord;
  SendSmsMapper: typeof SmsRecord;
};

// sms record dto types
export type SmsDtoTypes = {
  SmsRecordMapper: Schema<typeof SmsRecordMapper.schema, SchemaValidator>;
  SendSmsMapper: Schema<typeof SendSmsMapper.schema, SchemaValidator>;
};
