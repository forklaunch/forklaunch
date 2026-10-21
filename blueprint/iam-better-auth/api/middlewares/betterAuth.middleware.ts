import { NextFunction, Request, Response } from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import {
  ATTR_API_NAME,
  ATTR_CORRELATION_ID,
  ATTR_HTTP_REQUEST_METHOD,
  ATTR_HTTP_RESPONSE_STATUS_CODE,
  ATTR_HTTP_ROUTE,
  ATTR_SERVICE_NAME,
  httpRequestsTotalCounter,
  httpServerDurationHistogram
} from '@forklaunch/core/http';
import { context, trace } from '@opentelemetry/api';
import { betterAuth, BetterAuthOptions } from 'better-auth';
import { toNodeHandler } from 'better-auth/node';
import { withAuthEncryptionPolicy } from '../../domain/utils/authEncryptionPolicy.util';
import {
  Request as ExpressRequest,
  Response as ExpressResponse
} from 'express';
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
  // createContext already set the request's correlation id (from the
  // x-correlation-id header when the caller sent one) and returned it in the
  // response header; keep it, so logs and traces for this request line up.
  const correlationId = req.context.correlationId;
  span?.setAttribute(
    ATTR_API_NAME,
    `Better Auth: ${req.path.replace('/api/auth/', '').replace('/', '-')}`
  );
  span?.setAttribute(ATTR_CORRELATION_ID, correlationId);
  span?.setAttribute(ATTR_SERVICE_NAME, getEnvVar('OTEL_SERVICE_NAME'));

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
    // Scope covers reads, writes, transaction commits and session creation.
    // Only this identity handler uses the service-owned encryption policy.
    const startTime = process.hrtime();
    await withAuthEncryptionPolicy(() =>
      toNodeHandler(auth)(req as ExpressRequest, res as ExpressResponse)
    );

    const attributes = {
      [ATTR_SERVICE_NAME]: getEnvVar('OTEL_SERVICE_NAME'),
      [ATTR_API_NAME]: `Better Auth: ${req.path.replace('/api/auth/', '').replace('/', '-')}`,
      [ATTR_HTTP_REQUEST_METHOD]: req.method,
      [ATTR_HTTP_ROUTE]: req.originalPath ?? req.path,
      [ATTR_HTTP_RESPONSE_STATUS_CODE]: Number(res.statusCode) || 0
    };
    httpRequestsTotalCounter.add(1, attributes);
    // Same unit as core's request middleware (seconds), which this catch-all
    // route bypasses.
    const [seconds, nanoseconds] = process.hrtime(startTime);
    httpServerDurationHistogram.record(seconds + nanoseconds / 1e9, attributes);
  };
}
