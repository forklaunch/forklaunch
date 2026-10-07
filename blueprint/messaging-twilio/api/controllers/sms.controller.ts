import { handlers, IdSchema, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  SendSmsMapper,
  SmsRecordMapper
} from '../../domain/mappers/smsRecord.mappers';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.SmsService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const sendSms = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Send Sms',
    access: 'internal',
    summary: 'Send an sms message',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: SendSmsMapper.schema,
    responses: {
      200: SmsRecordMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Sending sms', req.body);
    res.status(200).json(await serviceFactory().sendSms(req.body));
  }
);

export const getSmsRecord = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Sms Record',
    access: 'internal',
    summary: 'Get an sms record',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: SmsRecordMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving sms record', req.params);
    res.status(200).json(await serviceFactory().getSmsRecord(req.params));
  }
);
