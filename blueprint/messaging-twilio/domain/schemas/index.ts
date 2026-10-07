import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import { TwilioSmsServiceSchemas } from '@forklaunch/implementation-messaging-twilio/schemas';

const schemas = mapServiceSchemas(
  {
    SmsSchemas: TwilioSmsServiceSchemas<SchemaValidator>
  },
  {
    uuidId: true,
    validator: schemaValidator
  }
);

export const { SmsSchemas } = schemas;
