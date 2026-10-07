import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import { BaseSmsServiceSchemas } from '@forklaunch/implementation-messaging-base/schemas';

const schemas = mapServiceSchemas(
  {
    SmsSchemas: BaseSmsServiceSchemas<SchemaValidator>
  },
  {
    uuidId: true,
    validator: schemaValidator
  }
);

export const { SmsSchemas } = schemas;
