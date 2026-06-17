import { getEnvVar } from '@forklaunch/common';
import { AnySchemaValidator } from '@forklaunch/validator';
import { context, trace } from '@opentelemetry/api';
import { ATTR_SERVICE_NAME as OTEL_ATTR_SERVICE_NAME } from '@opentelemetry/semantic-conventions';
import { v4 } from 'uuid';
import {
  ATTR_CORRELATION_ID,
  ATTR_SERVICE_NAME
} from '../../telemetry/constants';
import { httpRequestsInFlightCounter } from '../../telemetry/openTelemetryCollector';
import {
  ExpressLikeSchemaHandler,
  ForklaunchNextFunction,
  ResolvedForklaunchResponse,
  VersionedResponses
} from '../../types/apiDefinition.types';
import {
  Body,
  HeadersObject,
  ParamsObject,
  QueryObject,
  ResponsesObject,
  SessionObject,
  VersionSchema
} from '../../types/contractDetails.types';

/**
 * Middleware to create and add a request context.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template Request - A type that extends ForklaunchRequest.
 * @template Response - A type that extends ForklaunchResponse.
 * @template NextFunction - A type that extends ForklaunchNextFunction.
 * @param {SV} schemaValidator - The schema validator.
 * @returns {Function} - Middleware function to create request context.
 */
export function createContext<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends Body<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  LocalsObj extends Record<string, unknown>,
  VersionedApi extends VersionSchema<SV, 'middleware'>,
  SessionSchema extends SessionObject<SV>
>(
  schemaValidator: SV
): ExpressLikeSchemaHandler<
  SV,
  P,
  ResBodyMap,
  ReqBody,
  ReqQuery,
  ReqHeaders,
  ResHeaders,
  LocalsObj,
  VersionedApi,
  SessionSchema,
  unknown,
  unknown,
  ForklaunchNextFunction
> {
  return function setContext(req, res, next?) {
    req.schemaValidator = schemaValidator;

    let correlationId = v4();
    if (req.headers['x-correlation-id']) {
      correlationId = req.headers['x-correlation-id'] as string;
    }

    (
      res as ResolvedForklaunchResponse<
        ResHeaders,
        Record<string, string>,
        LocalsObj,
        VersionedResponses,
        unknown
      >
    ).setHeader('x-correlation-id', correlationId);

    req.context = {
      correlationId: correlationId,
      requestStartTime: Date.now()
    };

    httpRequestsInFlightCounter.add(1, {
      [OTEL_ATTR_SERVICE_NAME]: getEnvVar('OTEL_SERVICE_NAME')
    });

    const span = trace.getSpan(context.active());

    if (span != null) {
      req.context.span = span;
      req.context.span?.setAttribute(ATTR_CORRELATION_ID, correlationId);
      req.context.span?.setAttribute(
        ATTR_SERVICE_NAME,
        getEnvVar('OTEL_SERVICE_NAME')
      );
    }

    next?.();
  };
}
