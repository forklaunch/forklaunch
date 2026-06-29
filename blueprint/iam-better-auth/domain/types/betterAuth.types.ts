import {
  type NextFunction,
  type Request,
  type Response
} from '@forklaunch/blueprint-core';
import { Span } from '@opentelemetry/api';

export type BetterAuthRequest = Request & {
  path: string;
  originalPath?: string;

  context: {
    correlationId: string;
    span?: Span;
  };
};

export type BetterAuthResponse = Response;

export type BetterAuthNextFunction = NextFunction;
