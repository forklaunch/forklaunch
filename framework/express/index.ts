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
import { AnySchemaValidator } from '@forklaunch/validator';
import { RequestHandler } from 'express';
import { ParsedQs } from 'qs';
import { Application } from './src/expressApplication';
import { Router } from './src/expressRouter';
import { checkout } from './src/handlers/checkout';
import { copy } from './src/handlers/copy';
import { delete_ } from './src/handlers/delete';
import { get } from './src/handlers/get';
import { head } from './src/handlers/head';
import { link } from './src/handlers/link';
import { lock } from './src/handlers/lock';
import { mSearch } from './src/handlers/m-search';
import { merge } from './src/handlers/merge';
import { middleware } from './src/handlers/middleware';
import { mkcActivity } from './src/handlers/mkcactivity';
import { mkcol } from './src/handlers/mkcol';
import { move } from './src/handlers/move';
import { notify } from './src/handlers/notify';
import { options } from './src/handlers/options';
import { patch } from './src/handlers/patch';
import { post } from './src/handlers/post';
import { propfind } from './src/handlers/propfind';
import { proppatch } from './src/handlers/proppatch';
import { purge } from './src/handlers/purge';
import { put } from './src/handlers/put';
import { report } from './src/handlers/report';
import { search } from './src/handlers/search';
import { subscribe } from './src/handlers/subscribe';
import { trace } from './src/handlers/trace';
import { unlink } from './src/handlers/unlink';
import { unlock } from './src/handlers/unlock';
import { unsubscribe } from './src/handlers/unsubscribe';
import {
  ExpressApplicationOptions,
  ExpressRouterOptions
} from './src/types/expressOptions.types';

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
  const router = new Router<SV, BasePath, SessionSchema>(
    basePath,
    schemaValidator,
    openTelemetryCollector,
    options
  );
  return router;
}

export type { DocsConfiguration, ParsedQs } from '@forklaunch/core/http';
export type { OptionsJson, OptionsText, OptionsUrlencoded } from 'body-parser';
export type { BusboyConfig } from 'busboy';
export type { CorsOptions } from 'cors';
export type { NextFunction, Request, Response } from 'express';
export type * from 'express-serve-static-core';
export type { Application } from './src/expressApplication';
export type { Router } from './src/expressRouter';
export type {
  ExpressApplicationOptions,
  ExpressRouterOptions
} from './src/types/expressOptions.types';

export const handlers: {
  checkout: typeof checkout;
  copy: typeof copy;
  delete: typeof delete_;
  get: typeof get;
  head: typeof head;
  link: typeof link;
  lock: typeof lock;
  mSearch: typeof mSearch;
  merge: typeof merge;
  middleware: typeof middleware;
  mkcActivity: typeof mkcActivity;
  mkcol: typeof mkcol;
  move: typeof move;
  notify: typeof notify;
  options: typeof options;
  patch: typeof patch;
  post: typeof post;
  propfind: typeof propfind;
  proppatch: typeof proppatch;
  purge: typeof purge;
  put: typeof put;
  report: typeof report;
  search: typeof search;
  subscribe: typeof subscribe;
  trace: typeof trace;
  unlink: typeof unlink;
  unlock: typeof unlock;
  unsubscribe: typeof unsubscribe;
} = {
  checkout,
  copy,
  delete: delete_,
  get,
  head,
  link,
  lock,
  mSearch,
  merge,
  middleware,
  mkcActivity,
  mkcol,
  move,
  notify,
  options,
  patch,
  post,
  propfind,
  proppatch,
  purge,
  put,
  report,
  search,
  subscribe,
  trace,
  unlink,
  unlock,
  unsubscribe
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
): RequestHandler[] => {
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
    ...(handlers.middlewares as RequestHandler[]),
    handlers.controllerHandler as unknown as RequestHandler
  ];
};

type _Shim = ParsedQs;
