import {
  handlers,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.SmsService);

export const handleStatusCallback = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Handle Status Callback',
    access: 'public',
    summary: 'Handle a twilio delivery status callback via webhook',
    // Twilio posts delivery-status callbacks as
    // application/x-www-form-urlencoded
    body: {
      urlEncodedForm: {
        MessageSid: string,
        MessageStatus: string,
        ErrorMessage: optional(string)
      }
    },
    responses: {
      200: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Processing twilio status callback', {
      messageSid: req.body.MessageSid,
      messageStatus: req.body.MessageStatus
    });
    await serviceFactory().processStatusCallback(req.body);
    res.status(200).send('Status callback processed');
  }
);
