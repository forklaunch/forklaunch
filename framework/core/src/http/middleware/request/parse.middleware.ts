import { isRecord } from '@forklaunch/common';
import {
  AnySchemaValidator,
  ParseResult,
  prettyPrintParseErrors,
  SchemaValidator
} from '@forklaunch/validator';
import { ParsedQs } from 'qs';
import { hasSend } from '../../guards/hasSend';
import { isRequestShape } from '../../guards/isRequestShape';
import {
  ForklaunchNextFunction,
  ForklaunchRequest,
  ForklaunchResponse,
  VersionedRequests
} from '../../types/apiDefinition.types';
import { ParamsDictionary } from '../../types/contractDetails.types';

/**
 * Pre-handler function to parse and validate input.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template Request - A type that extends ForklaunchRequest.
 * @template Response - A type that extends ForklaunchResponse.
 * @template NextFunction - A type that extends ForklaunchNextFunction.
 * @param {Request} req - The request object.
 * @param {Response} res - The response object.
 * @param {NextFunction} [next] - The next middleware function.
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
    ResHeaders,
    LocalsObj,
    Extract<keyof VersionedReqs, string>
  >,
  next?: ForklaunchNextFunction
) {
  const globalOptions = req._globalOptions?.();
  const collapsedOptions =
    req.contractDetails.options?.requestValidation ??
    (globalOptions?.validation === false
      ? 'none'
      : globalOptions?.validation?.request);

  // Preserve the pre-validation body for HMAC verification when the content
  // parser did not already capture the exact request bytes. Never clobber a
  // parser-captured _rawBody: that value is the raw payload as sent by the
  // client, which signature verification (Stripe et al.) must be computed
  // over — req.body may already be a parsed object here.
  req._rawBody ??= req.body;

  const request = {
    params: req.params,
    query: req.query,
    headers: req.headers,
    body: req.body
  };

  const schemaValidator = req.schemaValidator as SchemaValidator;

  let matchedVersions;

  let parsedRequest: ParseResult<object> | undefined;
  let collectedParseErrors: string | undefined;

  if (req.contractDetails.versions) {
    if (isRecord(req.requestSchema)) {
      let runningParseErrors: string = '';
      matchedVersions = [];

      Object.entries(req.requestSchema).forEach(([version, schema]) => {
        const parsingResult = schemaValidator.parse(schema, request);

        if (parsingResult.ok) {
          parsedRequest = parsingResult;
          matchedVersions.push(version);
          req.version = version as Extract<keyof VersionedReqs, string>;
          res.version = req.version;
        } else {
          runningParseErrors += prettyPrintParseErrors(
            parsingResult.errors,
            `Version ${version} request`
          );
        }
      });

      if (!parsedRequest) {
        parsedRequest = {
          ok: false,
          errors: []
        };
        collectedParseErrors = runningParseErrors;
      }
    } else {
      req.version = Object.keys(req.contractDetails.versions).pop() as Extract<
        keyof VersionedReqs,
        string
      >;
      res.version = req.version;

      parsedRequest = {
        ok: true,
        value: request
      };

      matchedVersions = Object.keys(req.contractDetails.versions);
    }
  } else {
    const parsingResult = schemaValidator.parse(req.requestSchema, request);

    parsedRequest = parsingResult;

    matchedVersions = 0;
  }

  if (
    parsedRequest.ok &&
    isRequestShape<P, ReqHeaders, ReqQuery, ReqBody>(parsedRequest.value)
  ) {
    req.body = parsedRequest.value.body;
    req.params = parsedRequest.value.params;
    Object.defineProperty(req, 'query', {
      value: parsedRequest.value.query,
      writable: false,
      enumerable: true,
      configurable: false
    });
    const parsedHeaders = parsedRequest.value.headers ?? {};
    req.headers = Object.keys(req.headers).reduce<Record<string, unknown>>(
      (acc, key) => {
        if (parsedHeaders?.[key]) {
          acc[key] = parsedHeaders[key];
        } else {
          acc[key] = req.headers[key];
        }
        return acc;
      },
      {}
    ) as ReqHeaders;
  }
  if (!parsedRequest.ok) {
    switch (collapsedOptions) {
      default:
      case undefined:
      case 'error':
        res.type('application/json');
        res.status(400);
        if (hasSend(res)) {
          res.send(
            `${
              collectedParseErrors ??
              prettyPrintParseErrors(parsedRequest.errors, 'Request')
            }\n\nCorrelation id: ${
              req.context.correlationId ?? 'No correlation ID'
            }`
          );
        } else {
          next?.(new Error('Request is not sendable.'));
        }
        return;
      case 'warning':
        req.openTelemetryCollector?.warn(
          collectedParseErrors ??
            prettyPrintParseErrors(parsedRequest.errors, 'Request')
        );
        break;
      case 'none':
        break;
    }
  }

  req._parsedVersions = matchedVersions;
  next?.();
}
