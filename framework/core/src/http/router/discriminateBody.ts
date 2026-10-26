import { SchemaObject } from 'openapi3-ts/oas31';
import { AnySchemaValidator, SchemaValidator } from '@forklaunch/validator';
import {
  Body,
  ResponsesObject,
  TypedBody
} from '../types/contractDetails.types';

/**
 * Discriminates the body type from the given contract details and returns information
 * necessary for parsing and validation.
 *
 * @template SV - Schema validator type.
 * @template Path - Route path type.
 * @template P - Params schema type.
 * @template ResBodyMap - Response body map type.
 * @template ReqBody - Request body type.
 * @template ReqQuery - Request query object type.
 * @template ReqHeaders - Request headers object type.
 * @template ResHeaders - Response headers object type.
 * @param contractDetails - The HTTP contract details.
 * @returns An object containing the content type, parser type, and schema.
 * @throws If no body-related information is found in the contract details.
 */
export function discriminateBody<SV extends AnySchemaValidator>(
  schemaValidator: SV,
  body: Body<SV> | undefined,
  inputProjection?: SchemaObject
):
  | {
      contentType: string;
      parserType: 'json' | 'text' | 'multipart' | 'urlEncoded' | 'file';
      schema: SV['_ValidSchemaObject'];
    }
  | undefined {
  if (body == null) {
    return undefined;
  }

  const maybeTypedBody = body as TypedBody<SV>;

  if ('text' in maybeTypedBody && maybeTypedBody.text != null) {
    return {
      contentType: maybeTypedBody.contentType ?? 'text/plain',
      parserType: 'text',
      schema: maybeTypedBody.text
    };
  } else if ('json' in maybeTypedBody && maybeTypedBody.json != null) {
    return {
      contentType: maybeTypedBody.contentType ?? 'application/json',
      parserType: 'json',
      schema: maybeTypedBody.json
    };
  } else if ('file' in maybeTypedBody && maybeTypedBody.file != null) {
    return {
      contentType: maybeTypedBody.contentType ?? 'application/octet-stream',
      parserType: 'file',
      schema: maybeTypedBody.file
    };
  } else if (
    'multipartForm' in maybeTypedBody &&
    maybeTypedBody.multipartForm != null
  ) {
    return {
      contentType: maybeTypedBody.contentType ?? 'multipart/form-data',
      parserType: 'multipart',
      schema: maybeTypedBody.multipartForm
    };
  } else if (
    'urlEncodedForm' in maybeTypedBody &&
    maybeTypedBody.urlEncodedForm != null
  ) {
    return {
      contentType:
        maybeTypedBody.contentType ?? 'application/x-www-form-urlencoded',
      parserType: 'urlEncoded',
      schema: maybeTypedBody.urlEncodedForm
    };
  } else if ('schema' in maybeTypedBody && maybeTypedBody.schema != null) {
    return {
      contentType: maybeTypedBody.contentType ?? 'application/json',
      parserType: 'text',
      schema: maybeTypedBody.schema
    };
  } else if (
    (schemaValidator as SchemaValidator).isInstanceOf(
      maybeTypedBody,
      schemaValidator.string
    )
  ) {
    return {
      contentType: 'text/plain',
      parserType: 'text',
      schema: maybeTypedBody
    };
  } else if (
    (inputProjection ??
      (schemaValidator as SchemaValidator).openapi(maybeTypedBody)).format ===
    'binary'
  ) {
    return {
      contentType: 'application/octet-stream',
      parserType: 'file',
      schema: maybeTypedBody
    };
  } else {
    return {
      contentType: 'application/json',
      parserType: 'json',
      schema: maybeTypedBody
    };
  }
}

/**
 * Discriminates all response body types from the given contract details and returns
 * information necessary for parsing and validation for each status code.
 *
 * @template SV - Schema validator type.
 * @template Path - Route path type.
 * @template P - Params schema type.
 * @template ResBodyMap - Response body map type.
 * @template ReqQuery - Request query object type.
 * @template ReqHeaders - Request headers object type.
 * @template ResHeaders - Response headers object type.
 * @param contractDetails - The contract details containing response schemas.
 * @returns A record mapping status codes to content type, parser type, and schema info.
 */
export function discriminateResponseBodies<SV extends AnySchemaValidator>(
  schemaValidator: SV,
  responses: ResponsesObject<SV>
) {
  const discriminatedResponses: Record<
    number,
    {
      contentType: string;
      parserType: 'json' | 'text' | 'serverSentEvent' | 'file' | 'multipart';
      schema: SV['_ValidSchemaObject'];
    }
  > = {};
  for (const [statusCode, response] of Object.entries(responses)) {
    if (response != null && typeof response === 'object') {
      if ('json' in response && response.json != null) {
        discriminatedResponses[Number(statusCode)] = {
          contentType:
            ('contentType' in response &&
            typeof response.contentType === 'string'
              ? response.contentType
              : 'application/json') ?? 'application/json',
          parserType: 'json',
          schema: response.json
        };
      } else if ('schema' in response && response.schema != null) {
        discriminatedResponses[Number(statusCode)] = {
          contentType:
            ('contentType' in response &&
            typeof response.contentType === 'string'
              ? response.contentType
              : 'application/json') ?? 'application/json',
          parserType: 'text',
          schema: response.schema
        };
      } else if ('text' in response && response.text != null) {
        discriminatedResponses[Number(statusCode)] = {
          contentType:
            ('contentType' in response &&
            typeof response.contentType === 'string'
              ? response.contentType
              : 'text/plain') ?? 'text/plain',
          parserType: 'text',
          schema: response.text
        };
      } else if ('file' in response && response.file != null) {
        discriminatedResponses[Number(statusCode)] = {
          contentType:
            ('contentType' in response &&
            typeof response.contentType === 'string'
              ? response.contentType
              : 'application/octet-stream') ?? 'application/octet-stream',
          parserType: 'file',
          schema: response.file
        };
      } else if ('event' in response && response.event != null) {
        discriminatedResponses[Number(statusCode)] = {
          contentType:
            ('contentType' in response &&
            typeof response.contentType === 'string'
              ? response.contentType
              : 'text/event-stream') ?? 'text/event-stream',
          parserType: 'serverSentEvent',
          schema: response.event
        };
      } else if (
        (schemaValidator as SchemaValidator).isInstanceOf(
          response,
          schemaValidator.string
        )
      ) {
        discriminatedResponses[Number(statusCode)] = {
          contentType: 'text/plain',
          parserType: 'text',
          schema: response
        };
      } else if (
        (schemaValidator as SchemaValidator).openapi(response).format ===
        'binary'
      ) {
        discriminatedResponses[Number(statusCode)] = {
          contentType: 'application/octet-stream',
          parserType: 'file',
          schema: response
        };
      } else {
        discriminatedResponses[Number(statusCode)] = {
          contentType: 'application/json',
          parserType: 'json',
          schema: response
        };
      }
    } else {
      discriminatedResponses[Number(statusCode)] = {
        contentType: 'application/json',
        parserType: 'json',
        schema: response
      };
    }
  }
  return discriminatedResponses;
}
