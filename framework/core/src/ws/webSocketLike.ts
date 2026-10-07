import {
  AnySchemaValidator,
  IdiomaticSchema,
  prettyPrintParseErrors,
  SchemaValidator
} from '@forklaunch/validator';

import { EventSchema, EventSchemaEntry } from './types/eventSchema.types';

type SchemaRecord<SV extends AnySchemaValidator> =
  Record<string, EventSchemaEntry<SV>> | undefined;

export function buildUnionSchema<SV extends AnySchemaValidator>(
  schemaValidator: SchemaValidator,
  record: SchemaRecord<SV>
): IdiomaticSchema<SV> | undefined {
  if (!record) {
    return undefined;
  }

  const schemas = Object.values(record)
    .filter((entry): entry is EventSchemaEntry<SV> => Boolean(entry))
    .map((entry) => entry.shape);

  if (schemas.length === 0) {
    return undefined;
  }

  if (schemas.length === 1) {
    return schemas[0];
  }

  return schemaValidator.union(schemas) as IdiomaticSchema<SV>;
}

export function parseSchemaValue<SV extends AnySchemaValidator>(
  schemaValidator: SchemaValidator,
  value: unknown,
  schema: IdiomaticSchema<SV> | undefined,
  context: string
): unknown {
  if (!schema) {
    return value;
  }

  const result = schemaValidator.parse(schema, value);
  if (result.ok) {
    return result.value;
  }

  const errors = 'errors' in result ? result.errors || [] : [];
  throw new Error(prettyPrintParseErrors(errors, context));
}

export function encodeSchemaValue<SV extends AnySchemaValidator>(
  schemaValidator: SchemaValidator,
  value: unknown,
  schema: IdiomaticSchema<SV> | undefined,
  context: string
): unknown {
  const validated = parseSchemaValue(schemaValidator, value, schema, context);

  if (
    Buffer.isBuffer(validated) ||
    validated instanceof ArrayBuffer ||
    ArrayBuffer.isView(validated)
  ) {
    if (Buffer.isBuffer(validated)) {
      return validated;
    }

    if (validated instanceof ArrayBuffer) {
      return Buffer.from(validated);
    }

    const view = validated as {
      buffer: ArrayBuffer;
      byteOffset: number;
      byteLength: number;
    };
    const typed = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
    return Buffer.from(typed);
  }

  if (typeof validated === 'string') {
    return Buffer.from(validated, 'utf-8');
  }

  if (validated === null || validated === undefined) {
    return validated;
  }

  return Buffer.from(JSON.stringify(validated), 'utf-8');
}

export function decodeSchemaValue<SV extends AnySchemaValidator>(
  schemaValidator: SchemaValidator,
  data: unknown,
  schema: IdiomaticSchema<SV> | undefined,
  context: string
): unknown {
  let decoded: string;
  let parsed: unknown;

  if (Buffer.isBuffer(data)) {
    decoded = data.toString('utf-8');
  } else if (data instanceof ArrayBuffer) {
    decoded = Buffer.from(data).toString('utf-8');
  } else if (ArrayBuffer.isView(data)) {
    const view = data as {
      buffer: ArrayBuffer;
      byteOffset: number;
      byteLength: number;
    };
    decoded = Buffer.from(
      view.buffer,
      view.byteOffset,
      view.byteLength
    ).toString('utf-8');
  } else if (typeof data === 'string') {
    decoded = data;
  } else {
    parsed = data;
    return parseSchemaValue(schemaValidator, parsed, schema, context);
  }

  try {
    parsed = JSON.parse(decoded);
  } catch {
    parsed = decoded;
  }

  return parseSchemaValue(schemaValidator, parsed, schema, context);
}

export function normalizeEncodedValue(
  encoded: unknown,
  context: string,
  allowUndefined = false
): Buffer | undefined {
  if (encoded === null || encoded === undefined) {
    if (allowUndefined) {
      return undefined;
    }
    throw new Error(`Invalid ${context}`);
  }

  if (Buffer.isBuffer(encoded)) {
    return encoded;
  }

  if (encoded instanceof ArrayBuffer) {
    return Buffer.from(encoded);
  }

  if (ArrayBuffer.isView(encoded)) {
    const view = encoded as ArrayBufferView;
    return Buffer.from(view.buffer, view.byteOffset, view.byteLength);
  }

  if (typeof encoded === 'string') {
    return Buffer.from(encoded, 'utf-8');
  }

  if (
    typeof encoded === 'number' ||
    typeof encoded === 'boolean' ||
    typeof encoded === 'bigint'
  ) {
    return Buffer.from(JSON.stringify(encoded), 'utf-8');
  }

  if (typeof encoded === 'object') {
    return Buffer.from(JSON.stringify(encoded), 'utf-8');
  }

  throw new Error(`Unsupported payload type for ${context}`);
}

export function createWebSocketSchemas<
  SV extends AnySchemaValidator,
  ES extends EventSchema<SV>
>(schemaValidator: SchemaValidator, eventSchemas: ES) {
  const clientMessagesSchema = buildUnionSchema<SV>(
    schemaValidator,
    eventSchemas.clientMessages
  );
  const serverMessagesSchema = buildUnionSchema<SV>(
    schemaValidator,
    eventSchemas.serverMessages
  );
  const errorsSchema = buildUnionSchema<SV>(
    schemaValidator,
    eventSchemas.errors
  );
  const pingSchema = eventSchemas.ping?.shape;
  const pongSchema = eventSchemas.pong?.shape;
  const closeReasonSchema = eventSchemas.closeReason
    ? buildUnionSchema<SV>(schemaValidator, eventSchemas.closeReason)
    : undefined;

  return {
    clientMessagesSchema,
    serverMessagesSchema,
    errorsSchema,
    pingSchema,
    pongSchema,
    closeReasonSchema
  };
}
