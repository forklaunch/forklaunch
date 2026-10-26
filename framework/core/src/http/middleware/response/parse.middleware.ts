import {
  AnySchemaValidator,
  prettyPrintParseErrors,
  SchemaValidator
} from '@forklaunch/validator';
import { ParsedQs } from 'qs';
import { hasSend } from '../../guards/hasSend';
import { isResponseCompiledSchema } from '../../guards/isResponseCompiledSchema';
import {
  ForklaunchNextFunction,
  ForklaunchRequest,
  ForklaunchResHeaders,
  ForklaunchResponse,
  VersionedRequests
} from '../../types/apiDefinition.types';
import { ParamsDictionary } from '../../types/contractDetails.types';

// Sending the diagnostic response re-enters enriched send. Do not validate
// that framework-generated diagnostic against the failed application headers.
// Weak membership is per response and cleared even when the sender throws.
const sendingValidationError = new WeakSet<object>();

/**
 * Middleware to parse and validate the response according to the provided schema.
 *
 * This function validates the response against a schema provided by the request's schema validator.
 * If the response does not conform to the schema, the behavior is determined by the `responseValidation`
 * option in `req.contractDetails.options`:
 * - `'error'` (default): Calls `next` with an error.
 * - `'warning'`: Logs a warning to the console.
 * - `'none'`: Silently continues without action.
 *
 * @template SV - The type of the schema validator used in the request.
 * @template P - The type of the parameters dictionary used in the request.
 * @template ResBodyMap - A record type mapping status codes to response body types.
 * @template ReqBody - The type of the request body.
 * @template ReqQuery - The type of the parsed query string.
 * @template ReqHeaders - The type of the request headers.
 * @template ResHeaders - The type of the response headers, extended from `ForklaunchResHeaders`.
 * @template LocalsObj - The type of the locals object in the response.
 *
 * @param {ForklaunchRequest<SV, P, ReqBody, ReqQuery, ReqHeaders>} req - The request object, containing the schema validator and other request data.
 * @param {ForklaunchResponse<ResBodyMap, ForklaunchResHeaders & ResHeaders, LocalsObj>} res - The response object, including headers and body data.
 * @param {ForklaunchNextFunction} [next] - The next middleware function to be called. If not provided, the function will return after processing.
 *
 * @returns {void} This function does not return a value.
 */
export function parse<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ResBodyMap extends Record<number, unknown>,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends ParsedQs,
  ReqHeaders extends Record<string, string>,
  ResHeaders extends Record<string, unknown>,
  LocalsObj extends Record<string, unknown>,
  VersionedReqs extends VersionedRequests,
  SessionSchema extends Record<string, unknown>
>(
  req: ForklaunchRequest<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    Extract<keyof VersionedReqs, string>,
    SessionSchema
  >,
  res: ForklaunchResponse<
    unknown,
    ResBodyMap,
    ForklaunchResHeaders & ResHeaders,
    LocalsObj,
    Extract<keyof VersionedReqs, string>
  >,
  next?: ForklaunchNextFunction
) {
  if (sendingValidationError.has(res)) {
    next?.();
    return;
  }
  let headers;
  let responses;

  const globalOptions = req._globalOptions?.();
  const collapsedOptions =
    req.contractDetails.options?.responseValidation ??
    (globalOptions?.validation === false
      ? 'none'
      : globalOptions?.validation?.response);

  if (collapsedOptions === 'none') {
    next?.();
    return;
  }

  const responseSchemas = res.responseSchemas;
  const schemaValidator = req.schemaValidator as SchemaValidator;

  if (!isResponseCompiledSchema(responseSchemas)) {
    const parsedVersions = req._parsedVersions;
    if (typeof parsedVersions === 'number') {
      throw new Error('Request failed to parse given version map');
    }
    const mappedHeaderSchemas = parsedVersions.map(
      (version) => responseSchemas[version].headers
    );
    const mappedResponseSchemas = parsedVersions.map(
      (version) => responseSchemas[version].responses
    );

    const collapsedResponseSchemas = mappedResponseSchemas.reduce<
      Record<number, unknown[]>
    >((acc, responseSchema) => {
      Object.entries(responseSchema).forEach(([status, schema]) => {
        if (!acc[Number(status)]) {
          acc[Number(status)] = [];
        }
        acc[Number(status)].push(schema);
      });
      return acc;
    }, {});

    headers = schemaValidator.union(mappedHeaderSchemas);
    responses = Object.fromEntries(
      Object.entries(collapsedResponseSchemas).map(([status, schemas]) => [
        status,
        schemaValidator.union(schemas)
      ])
    );
  } else {
    headers = responseSchemas.headers;
    responses = responseSchemas.responses;
  }

  const statusCode = Number(res.statusCode);

  const defaultErrorStatus = [400, 401, 404, 403, 500].includes(statusCode);
  const missingResponseSchema =
    !defaultErrorStatus &&
    !Object.prototype.hasOwnProperty.call(responses, statusCode);
  // A missing declaration is a contract diagnostic, not a schema to compile.
  // Preserve final-error redaction in error mode; warning mode must still send.
  if (missingResponseSchema && collapsedOptions !== 'warning') {
    throw new Error(`Undocumented response status ${statusCode}`);
  }
  // A default error status may carry its declared body or the framework's
  // string message. TypeBox cannot put a compiled schema in a union (building
  // one throws), so for a declared schema the two are tried in turn instead.
  const declaredResponse = responses?.[statusCode];
  const defaultErrorResponse = () => {
    try {
      return schemaValidator.parse(
        schemaValidator.union([schemaValidator.string, declaredResponse]),
        res.bodyData
      );
    } catch (error) {
      if (declaredResponse === undefined) throw error;
      const declared = schemaValidator.parse(declaredResponse, res.bodyData);
      if (declared.ok) return declared;
      const message = schemaValidator.parse(
        schemaValidator.string,
        res.bodyData
      );
      return message.ok ? message : declared;
    }
  };
  const parsedResponse = missingResponseSchema
    ? { ok: true as const }
    : defaultErrorStatus
      ? defaultErrorResponse()
      : schemaValidator.parse(declaredResponse, res.bodyData);

  const parsedHeaders = schemaValidator.parse(
    headers ?? schemaValidator.unknown,
    res.getHeaders()
  );
  const parseErrors: string[] = missingResponseSchema
    ? [`Response: Undocumented response status ${statusCode}`]
    : [];
  if (!parsedHeaders.ok) {
    const headerErrors = prettyPrintParseErrors(parsedHeaders.errors, 'Header');
    if (headerErrors) {
      parseErrors.push(headerErrors);
    }
  }

  if (!parsedResponse.ok) {
    const responseErrors = prettyPrintParseErrors(
      parsedResponse.errors,
      'Response'
    );
    if (responseErrors) {
      parseErrors.push(responseErrors);
    }
  }

  if (parseErrors.length > 0) {
    switch (collapsedOptions) {
      default:
      case undefined:
      case 'error':
        res.type('text/plain');
        res.status(500);
        if (hasSend(res)) {
          sendingValidationError.add(res);
          try {
            res.send(
              `Invalid response:\n${parseErrors.join(
                '\n\n'
              )}\n\nCorrelation id: ${
                req.context.correlationId ?? 'No correlation ID'
              }`
            );
          } finally {
            sendingValidationError.delete(res);
          }
        } else {
          next?.(new Error('Response is not sendable.'));
        }
        return;
      case 'warning':
        req.openTelemetryCollector?.warn(
          `Invalid response:\n${parseErrors.join('\n\n')}`
        );
        break;
    }
  }
  next?.();
}
