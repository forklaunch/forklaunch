import {
  type NextFunction,
  type Request,
  type Response
} from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import {
  ATTR_API_NAME,
  ATTR_CORRELATION_ID,
  ATTR_HTTP_REQUEST_METHOD,
  ATTR_HTTP_RESPONSE_STATUS_CODE,
  ATTR_HTTP_ROUTE,
  ATTR_SERVICE_NAME,
  httpRequestsTotalCounter
} from '@forklaunch/core/http';
import { context, trace } from '@opentelemetry/api';
import { betterAuth, BetterAuthOptions } from 'better-auth';
import { toNodeHandler } from 'better-auth/node';
import {
  Request as ExpressRequest,
  Response as ExpressResponse
} from 'express';
import { v4 } from 'uuid';
import { isBetterAuthRequest } from '../../domain/guards/isBetterAuthRequest.guard';

export function betterAuthTelemetryHookMiddleware(
  req: Request,
  _res: Response,
  next: NextFunction
) {
  if (!isBetterAuthRequest(req)) {
    throw new Error('Invalid request');
  }

  const span = trace.getSpan(context.active());
  const correlationId = v4();
  span?.setAttribute(
    ATTR_API_NAME,
    `Better Auth: ${req.path.replace('/api/auth/', '').replace('/', '-')}`
  );
  span?.setAttribute(ATTR_CORRELATION_ID, correlationId);
  span?.setAttribute(ATTR_SERVICE_NAME, getEnvVar('OTEL_SERVICE_NAME'));

  req.context.correlationId = correlationId;
  req.context.span = span;

  next();
}

export function enrichBetterAuthApi<T extends BetterAuthOptions>(
  auth: ReturnType<typeof betterAuth<T>>
) {
  return async (req: Request, res: Response) => {
    if (!isBetterAuthRequest(req)) {
      throw new Error('Invalid request');
    }
    await toNodeHandler(auth)(
      req as unknown as ExpressRequest,
      res as unknown as ExpressResponse
    );

    httpRequestsTotalCounter.add(1, {
      [ATTR_SERVICE_NAME]: getEnvVar('OTEL_SERVICE_NAME'),
      [ATTR_API_NAME]: `Better Auth: ${req.path.replace('/api/auth/', '').replace('/', '-')}`,
      [ATTR_HTTP_REQUEST_METHOD]: req.method,
      [ATTR_HTTP_ROUTE]: req.originalPath ?? req.path,
      [ATTR_HTTP_RESPONSE_STATUS_CODE]: Number(res.statusCode) || 0
    });
  };
}
