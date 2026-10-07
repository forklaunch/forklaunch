import {
  ContractDetailsOrMiddlewareOrTypedHandler,
  extractRouteHandlers,
  HeadersObject,
  Body as HttpBody,
  Method,
  MetricsDefinition,
  MiddlewareOrMiddlewareWithTypedHandler,
  OpenTelemetryCollector,
  ParamsObject,
  QueryObject,
  ResponsesObject,
  SchemaAuthMethods,
  SessionObject,
  VersionSchema
} from '@forklaunch/core/http';
import { MiddlewareHandler } from '@forklaunch/hyper-express-fork';
import { AnySchemaValidator } from '@forklaunch/validator';
import { any } from './src/handlers/any';
import { delete_ } from './src/handlers/delete';
import { get } from './src/handlers/get';
import { head } from './src/handlers/head';
import { middleware } from './src/handlers/middleware';
import { options } from './src/handlers/options';
import { patch } from './src/handlers/patch';
import { post } from './src/handlers/post';
import { put } from './src/handlers/put';
import { trace } from './src/handlers/trace';
import { Application } from './src/hyperExpressApplication';
import { Router } from './src/hyperExpressRouter';
import {
  ExpressApplicationOptions,
  ExpressRouterOptions
} from './src/types/hyperExpressOptions.types';

export type App<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>
> = Application<SV, SessionSchema>;

/**
 * Creates a new instance of Application with the given schema validator.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @param {SV} schemaValidator - The schema validator.
 * @returns {Application<SV>} - The new application instance.
 */
export function forklaunchExpress<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>
>(
  schemaValidator: SV,
  openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
  options?: ExpressApplicationOptions<SV, SessionSchema>
) {
  return new Application<SV, SessionSchema>(
    schemaValidator,
    openTelemetryCollector,
    options
  );
}

/**
 * Creates a new instance of Router with the given base path and schema validator.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @param {string} basePath - The base path for the router.
 * @param {SV} schemaValidator - The schema validator.
 * @returns {Router<SV>} - The new router instance.
 */
export function forklaunchRouter<
  SV extends AnySchemaValidator,
  BasePath extends `/${string}`,
  SessionSchema extends SessionObject<SV>
>(
  basePath: BasePath,
  schemaValidator: SV,
  openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
  options?: ExpressRouterOptions<SV, SessionSchema>
): Router<SV, BasePath, SessionSchema> {
  const router = new Router(
    basePath,
    schemaValidator,
    openTelemetryCollector,
    options
  );
  return router;
}

export type {
  MiddlewareNext as NextFunction,
  Request,
  Response,
  ServerConstructorOptions
} from '@forklaunch/hyper-express-fork';
export type { BusboyConfig } from 'busboy';
export type { CorsOptions } from 'cors';
export type { Application } from './src/hyperExpressApplication';
export type { Router } from './src/hyperExpressRouter';
export type {
  ExpressApplicationOptions,
  ExpressRouterOptions
} from './src/types/hyperExpressOptions.types';

export const handlers: {
  any: typeof any;
  delete: typeof delete_;
  get: typeof get;
  head: typeof head;
  middleware: typeof middleware;
  options: typeof options;
  patch: typeof patch;
  post: typeof post;
  put: typeof put;
  trace: typeof trace;
} = {
  any,
  delete: delete_,
  get,
  head,
  middleware,
  options,
  patch,
  post,
  put,
  trace
};

export const port = <
  SV extends AnySchemaValidator,
  Name extends string,
  ContractMethod extends Method,
  Path extends `/${string}`,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends HttpBody<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  LocalsObj extends Record<string, unknown>,
  VersionedApi extends VersionSchema<SV, ContractMethod>,
  RouterSession extends SessionObject<SV>,
  BaseRequest,
  BaseResponse,
  NextFunction,
  Auth extends SchemaAuthMethods<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >,
  RouterHandler
>(
  options: {
    method: ContractMethod;
    path: Path;
    basePath: string;
    schemaValidator: SV;
    openTelemetryCollector?: OpenTelemetryCollector<MetricsDefinition>;
  },
  contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
    SV,
    Name,
    ContractMethod,
    Path,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedApi,
    BaseRequest,
    BaseResponse,
    NextFunction,
    RouterSession,
    Auth
  >,
  ...middlewareOrMiddlewareAndTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
    SV,
    Name,
    ContractMethod,
    Path,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedApi,
    BaseRequest,
    BaseResponse,
    NextFunction,
    RouterSession,
    Auth
  >[]
): MiddlewareHandler[] => {
  const handlers = extractRouteHandlers<
    SV,
    Name,
    ContractMethod,
    Path,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedApi,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    Auth,
    RouterHandler
  >({
    ...options,
    contractDetailsOrMiddlewareOrTypedHandler,
    middlewareOrMiddlewareAndTypedHandler
  });

  return [
    ...(handlers.middlewares as MiddlewareHandler[]),
    // A controller handler is typed against the framework's own request/response
    // shapes; uWebSockets' `MiddlewareHandler` names its own. The runtime object
    // is the same function — only the two signatures fail to overlap.
    handlers.controllerHandler as unknown as MiddlewareHandler
  ];
};
