import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { getSmsRecord, sendSms } from '../controllers/sms.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const smsRouter = forklaunchRouter(
  '/sms',
  schemaValidator,
  openTelemetryCollector
);

export const sendSmsRoute = smsRouter.post('/', sendSms);
export const getSmsRecordRoute = smsRouter.get('/:id', getSmsRecord);
