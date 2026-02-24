import { AnySchemaValidator, SchemaValidator } from '@forklaunch/validator';
import { ParsedQs } from 'qs';

import {
  EmptyObject,
  hashString,
  Prettify,
  PrettyCamelCase,
  safeStringify,
  sanitizePathSlashes,
  SanitizePathSlashes,
  toPrettyCamelCase,
  toRecord,
  TypeSafeFunction
} from '@forklaunch/common';
import { isConstrainedForklaunchRouter } from '../guards/isConstrainedForklaunchRouter';
import { isExpressLikeSchemaHandler } from '../guards/isExpressLikeSchemaHandler';
import { isForklaunchExpressLikeRouter } from '../guards/isForklaunchExpressLikeRouter';
import { isForklaunchRouter } from '../guards/isForklaunchRouter';
import { isSdkHandler } from '../guards/isSdkHandler';
import { isTypedHandler } from '../guards/isTypedHandler';
import {
  ExpressLikeRouter,
  NestableRouterBasedHandler,
  PathBasedHandler,
  PathOrMiddlewareBasedHandler
} from '../interfaces/expressLikeRouter.interface';
import { createContext } from '../middleware/request/createContext.middleware';
import { OpenTelemetryCollector } from '../telemetry/openTelemetryCollector';
import {
  ExpressLikeHandler,
  ExpressLikeSchemaHandler,
  LiveSdkFunction,
  LiveTypeFunction,
  VersionedRequests,
  VersionedResponses
} from '../types/apiDefinition.types';
import {
  Body,
  HeadersObject,
  Method,
  ParamsDictionary,
  ParamsObject,
  PathParamHttpContractDetails,
  QueryObject,
  ResponsesObject,
  SchemaAuthMethods,
  SessionObject,
  VersionSchema
} from '../types/contractDetails.types';
import { ExpressLikeRouterOptions } from '../types/expressLikeOptions';
import {
  ContractDetailsOrMiddlewareOrTypedHandler,
  LiveTypeRouteDefinition,
  MiddlewareOrMiddlewareWithTypedHandler,
  TypedMiddlewareDefinition,
  TypedNestableMiddlewareDefinition
} from '../types/expressLikeRouter.types';
import { MetricsDefinition } from '../types/openTelemetryCollector.types';
import {
  ConstrainedForklaunchRouter,
  ForklaunchRoute,
  ForklaunchRouter
} from '../types/router.types';
import { FetchFunction, SdkHandlerObject } from '../types/sdk.types';
import { discriminateBody } from './discriminateBody';
import {
  compileRouteSchemas,
  resolveContractDetailsAndHandlers,
  resolveRouteMiddlewares,
  validateContractDetails
} from './routerSharedLogic';

/**
 * Extracts route middleware and handlers from route registration arguments.
 * This is a port function that converts typed handlers into Express middleware
 * without actually registering them on a router.
 *
 * @template SV - The schema validator type.
 * @template Name - The route name.
 * @template ContractMethod - The HTTP method.
 * @template Path - The route path.
 * @template P - The type of request parameters.
 * @template ResBodyMap - The type of response body map.
 * @template ReqBody - The type of request body.
 * @template ReqQuery - The type of request query.
 * @template ReqHeaders - The type of request headers.
 * @template ResHeaders - The type of response headers.
 * @template LocalsObj - The type of local variables.
 * @template VersionedApi - The versioned API schema.
 * @template RouterSession - The router session type.
 * @template BaseRequest - The base request type.
 * @template BaseResponse - The base response type.
 * @template NextFunction - The next function type.
 * @template Auth - The auth schema.
 * @template RouterHandler - The router handler type.
 * @param {object} params - The parameters for extracting route handlers.
 * @param {ContractMethod} params.method - The HTTP method.
 * @param {Path} params.path - The route path.
 * @param {string} params.basePath - The base path for the router.
 * @param {SV} params.schemaValidator - The schema validator.
 * @param {ContractDetailsOrMiddlewareOrTypedHandler<SV, Name, ContractMethod, Path, P, ResBodyMap, ReqBody, ReqQuery, ReqHeaders, ResHeaders, LocalsObj, VersionedApi, BaseRequest, BaseResponse, NextFunction, RouterSession, Auth>} params.contractDetailsOrMiddlewareOrTypedHandler - The contract details or typed handler.
 * @param {MiddlewareOrMiddlewareWithTypedHandler<SV, Name, ContractMethod, Path, P, ResBodyMap, ReqBody, ReqQuery, ReqHeaders, ResHeaders, LocalsObj, VersionedApi, BaseRequest, BaseResponse, NextFunction, RouterSession, Auth>[]} params.middlewareOrMiddlewareAndTypedHandler - Additional middleware or typed handlers.
 * @param {OpenTelemetryCollector<MetricsDefinition>} params.openTelemetryCollector - The OpenTelemetry collector.
 * @param {RouterHandler[]} params.postEnrichMiddleware - Additional middleware to run after enrichment.
 * @param {ExpressLikeRouterOptions<SV, RouterSession> | undefined} params.routerOptions - Router options.
 * @returns {{ middlewares: RouterHandler[]; controllerHandler: ExpressLikeHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, ReqHeaders, ResHeaders, LocalsObj, VersionedRequests, VersionedResponses, RouterSession, BaseRequest, BaseResponse, NextFunction> }} - The extracted middlewares and controller handler.
 */
export function extractRouteHandlers<
  SV extends AnySchemaValidator,
  Name extends string,
  ContractMethod extends Method,
  Path extends `/${string}`,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends Body<SV>,
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
>(params: {
  method: ContractMethod;
  path: Path;
  basePath: string;
  schemaValidator: SV;
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
  >;
  middlewareOrMiddlewareAndTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
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
  >[];
  openTelemetryCollector?: OpenTelemetryCollector<MetricsDefinition>;
  postEnrichMiddleware?: RouterHandler[];
  routerOptions?: ExpressLikeRouterOptions<SV, RouterSession>;
}): {
  middlewares: RouterHandler[];
  controllerHandler: ExpressLikeSchemaHandler<
    SV,
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
    NextFunction
  >;
} {
  const schemaValidator = params.schemaValidator as SV & SchemaValidator;

  const { contractDetails, handlers } = resolveContractDetailsAndHandlers<
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
    Auth
  >(
    params.contractDetailsOrMiddlewareOrTypedHandler,
    params.middlewareOrMiddlewareAndTypedHandler
  );

  validateContractDetails(contractDetails, schemaValidator);

  const { requestSchema, responseSchemas } = compileRouteSchemas(
    contractDetails,
    schemaValidator
  );

  const { middlewares, controllerHandler } = resolveRouteMiddlewares<
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
    basePath: params.basePath,
    path: params.path,
    contractDetails,
    requestSchema,
    responseSchemas,
    openTelemetryCollector: params.openTelemetryCollector,
    routerOptions: params.routerOptions,
    postEnrichMiddleware: params.postEnrichMiddleware ?? [],
    handlers,
    includeCreateContext: false
  });

  return {
    middlewares: [
      createContext(schemaValidator) as RouterHandler,
      ...middlewares
    ],
    controllerHandler
  };
}

/**
 * A class that represents an Express-like router.
 */
export class ForklaunchExpressLikeRouter<
  SV extends AnySchemaValidator,
  BasePath extends `/${string}`,
  RouterHandler,
  Internal extends ExpressLikeRouter<RouterHandler, Internal>,
  BaseRequest,
  BaseResponse,
  NextFunction,
  RouterSession extends SessionObject<SV>,
  FetchMap extends Record<string, unknown> = EmptyObject,
  Sdk extends Record<string, unknown> = EmptyObject
> implements ConstrainedForklaunchRouter<SV, RouterHandler> {
  requestHandler!: RouterHandler;
  routers: ForklaunchRouter<SV>[] = [];
  routes: ForklaunchRoute<SV>[] = [];

  _fetchMap: FetchMap = {} as FetchMap;
  sdk: Sdk = {} as Sdk;

  sdkPaths: Record<string, string> = {};

  constructor(
    readonly basePath: BasePath,
    readonly schemaValidator: SV,
    readonly internal: Internal,
    readonly postEnrichMiddleware: RouterHandler[],
    readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    readonly routerOptions?: ExpressLikeRouterOptions<SV, RouterSession>
  ) {
    if (
      process.env.NODE_ENV !== 'test' &&
      !process.env.VITEST &&
      process.env.FORKLAUNCH_MODE !== 'openapi'
    ) {
      process.on('uncaughtException', (err) => {
        this.openTelemetryCollector.error(`Uncaught exception: ${err}`);
        process.exit(1);
      });
      process.on('unhandledRejection', (reason) => {
        this.openTelemetryCollector.error(`Unhandled rejection: ${reason}`);
        process.exit(1);
      });

      process.on('exit', () => {
        this.openTelemetryCollector.info('Shutting down application');
      });
      process.on('SIGINT', () => {
        this.openTelemetryCollector.info('Shutting down application');
        process.exit(0);
      });
    }

    this.internal.use(createContext(this.schemaValidator) as RouterHandler);
  }

  /**
   * Resolves middlewares based on the contract details.
   *
   * @param {PathParamHttpContractDetails<SV> | HttpContractDetails<SV>} contractDetails - The contract details.
   * @returns {MiddlewareHandler<SV>[]} - The resolved middlewares.
   */

  /**
   * Parses and runs the controller handler with error handling.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @template StatusCode - The type of status code.
   * @param {MiddlewareHandler<SV, P, ResBodyMap | string, ReqBody, ReqQuery, LocalsObj, StatusCode>} requestHandler - The request handler.
   * @returns {ExpressMiddlewareHandler} - The Express request handler.
   */
  #parseAndRunControllerHandler<
    P extends ParamsDictionary,
    ResBodyMap extends Record<number, unknown>,
    ReqBody extends Record<string, unknown>,
    ReqQuery extends ParsedQs,
    ReqHeaders extends Record<string, string>,
    ResHeaders extends Record<string, string>,
    LocalsObj extends Record<string, unknown>,
    VersionedReqs extends VersionedRequests,
    VersionedResps extends VersionedResponses,
    RouterSession extends Record<string, unknown>
  >(
    requestHandler: ExpressLikeHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedReqs,
      VersionedResps,
      RouterSession,
      BaseRequest,
      BaseResponse,
      NextFunction
    >
  ): ExpressLikeHandler<
    SV,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedReqs,
    VersionedResps,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction
  > {
    return async (req, res, next) => {
      if (!requestHandler) {
        throw new Error('Controller handler is not defined');
      }

      try {
        await requestHandler(req, res, next);
      } catch (error) {
        if (next && typeof next === 'function') {
          next(error as Error);
        } else {
          throw error;
        }
      }
    };
  }

  /**
   * Extracts the controller handler from the provided handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {MiddlewareHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The provided handlers.
   * @returns {MiddlewareHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>} - The extracted controller handler.
   * @throws {Error} - Throws an error if the last argument is not a handler.
   */

  /**
   * Fetches a route from the route map and executes it with the given parameters.
   *
   * @template Path - The path type that extends keyof _fetchMap and string.
   * @param {Path} path - The route path
   * @param {Parameters<_fetchMap[Path]>[1]} [requestInit] - Optional request initialization parameters.
   * @returns {Promise<ReturnType<_fetchMap[Path]>>} - The result of executing the route handler.
   */
  fetch: FetchFunction<this['_fetchMap']> = async <
    Path extends keyof this['_fetchMap'],
    Method extends keyof this['_fetchMap'][Path],
    Version extends keyof this['_fetchMap'][Path][Method]
  >(
    path: Path,
    ...reqInit: this['_fetchMap'][Path][Method] extends TypeSafeFunction
      ? 'GET' extends keyof this['_fetchMap'][Path]
        ? this['_fetchMap'][Path]['GET'] extends TypeSafeFunction
          ? Parameters<this['_fetchMap'][Path]['GET']>[1] extends
              | {
                  body: unknown;
                }
              | { query: unknown }
              | { params: unknown }
              | { headers: unknown }
              | { version: unknown }
            ? [
                reqInit: Omit<
                  Parameters<this['_fetchMap'][Path][Method]>[1],
                  'method'
                > & {
                  method: Method;
                }
              ]
            : [
                reqInit?: Omit<
                  Parameters<this['_fetchMap'][Path][Method]>[1],
                  'method'
                > & {
                  method: Method;
                }
              ]
          : [
              reqInit: Omit<
                Parameters<this['_fetchMap'][Path][Method]>[1],
                'method'
              > & {
                method: Method;
              }
            ]
        : [
            reqInit: Omit<
              Parameters<this['_fetchMap'][Path][Method]>[1],
              'method'
            > & {
              method: Method;
            }
          ]
      : this['_fetchMap'][Path][Method] extends Record<string, TypeSafeFunction>
        ? [
            reqInit: Omit<
              Parameters<this['_fetchMap'][Path][Method][Version]>[0],
              'method' | 'version'
            > & {
              method: Method;
              version: Version;
            }
          ]
        : [{ method: Method }]
  ) => {
    const method = reqInit[0]?.method;
    const version =
      reqInit[0] != null && 'version' in reqInit[0]
        ? reqInit[0].version
        : undefined;

    return (
      (version
        ? this._fetchMap[path][method ?? ('GET' as Method)][version]
        : this._fetchMap[path][method ?? ('GET' as Method)]) as (
        route: string,
        request?: unknown
      ) => unknown
    )(
      path as string,
      reqInit[0]
      // reqInit
    ) as this['_fetchMap'][Path][Method] extends TypeSafeFunction
      ? Awaited<ReturnType<this['_fetchMap'][Path][Method]>>
      : this['_fetchMap'][Path][Method] extends Record<string, TypeSafeFunction>
        ? Awaited<ReturnType<this['_fetchMap'][Path][Method][Version]>>
        : never;
  };

  /**
   * Executes request locally, applying parameters
   *
   * @param handlers {ExpressLikeHandler<SV>}
   * @param controllerHandler
   * @returns
   */
  #localParamRequest<Middleware, Route extends string>(
    middlewares: Middleware[],
    controllerHandler: Middleware,
    contractDetails: PathParamHttpContractDetails<SV>,
    requestSchema: unknown,
    responseSchemas: unknown,
    version?: string
  ) {
    return async (
      route: SanitizePathSlashes<Route>,
      request?: {
        params?: Record<string, unknown>;
        query?: Record<string, unknown>;
        headers?: Record<string, unknown>;
        body?: Record<string, unknown>;
        executeMiddlewares?: boolean;
      }
    ) => {
      let statusCode: number | undefined;
      let responseMessage: unknown;
      const responseHeaders: Record<string, unknown> = {};
      const resEventHandlers: Record<string, ((...args: unknown[]) => void)[]> =
        {};

      const req = {
        params: request?.params ?? {},
        query: request?.query ?? {},
        headers: request?.headers ?? {},
        body:
          discriminateBody(this.schemaValidator, request?.body)?.schema ?? {},
        path: route,
        originalPath: route,
        method: 'GET',
        version,
        schemaValidator: this.schemaValidator,
        contractDetails,
        requestSchema,
        openTelemetryCollector: this.openTelemetryCollector,
        _globalOptions: () => this.routerOptions,
        context: {
          correlationId: 'local-fetch-' + Date.now(),
          span: undefined
        },
        _rawBody: undefined as unknown,
        _parsedVersions: undefined as unknown
      };

      const res = {
        statusCode: 200,
        status: (code: number) => {
          statusCode = code;
          res.statusCode = code;
          return res;
        },
        send: (message: string) => {
          responseMessage = message;
          resEventHandlers['finish']?.forEach((handler) => handler());
        },
        json: (body: Record<string, unknown>) => {
          responseMessage = body;
          resEventHandlers['finish']?.forEach((handler) => handler());
        },
        jsonp: (body: Record<string, unknown>) => {
          responseMessage = body;
          resEventHandlers['finish']?.forEach((handler) => handler());
        },
        setHeader: (key: string, value: string) => {
          responseHeaders[key] = value;
          return res;
        },
        getHeader: (key: string) => responseHeaders[key],
        type: (contentType: string) => {
          responseHeaders['content-type'] = contentType;
          return res;
        },
        sseEmitter: (
          generator: () => AsyncGenerator<Record<string, unknown>>
        ) => {
          responseMessage = generator();
        },
        on: (event: string, handler: (...args: unknown[]) => void) => {
          if (!resEventHandlers[event]) {
            resEventHandlers[event] = [];
          }
          resEventHandlers[event].push(handler);
          return res;
        },
        responseSchemas,
        version
      };

      const executeMiddlewares = request?.executeMiddlewares ?? false;

      if (executeMiddlewares && middlewares.length > 0) {
        const allHandlers = [...middlewares];
        let cursor = allHandlers.shift() as unknown as (
          req_: typeof req,
          resp_: typeof res,
          next: (err?: Error) => Promise<void> | void
        ) => Promise<void> | void;

        if (cursor) {
          for (const fn of allHandlers) {
            await cursor(req, res, (err?: Error) => {
              if (err) {
                throw err;
              }
              cursor = fn as unknown as (
                req_: typeof req,
                resp_: typeof res,
                next: (err?: Error) => Promise<void> | void
              ) => Promise<void> | void;
            });
          }
          await cursor(req, res, async (err?: Error) => {
            if (err) {
              throw err;
            }
          });
        }
      }

      const cHandler = controllerHandler as unknown as (
        req_: typeof req,
        resp_: typeof res,
        next: (err?: Error) => Promise<void> | void
      ) => void;
      await cHandler(req, res, (err?: Error) => {
        if (err) {
          throw err;
        }
      });

      return {
        code: Number(statusCode),
        response: responseMessage,
        headers: responseHeaders
      };
    };
  }

  registerRoute<
    Name extends string,
    ContractMethod extends Method,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, ContractMethod>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    method: ContractMethod,
    path: Path,
    registrationMethod: PathBasedHandler<RouterHandler>,
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
  ): this & {
    _fetchMap: Prettify<
      FetchMap extends Record<
        SanitizePathSlashes<`${BasePath}${Path}`>,
        unknown
      >
        ? FetchMap &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              FetchMap[SanitizePathSlashes<`${BasePath}${Path}`>] &
                Record<
                  Uppercase<ContractMethod>,
                  LiveTypeFunction<
                    SV,
                    SanitizePathSlashes<`${BasePath}${Path}`>,
                    P,
                    ResBodyMap,
                    ReqBody,
                    ReqQuery,
                    ReqHeaders,
                    ResHeaders,
                    ContractMethod,
                    VersionedApi,
                    Auth
                  >
                >
            >
        : FetchMap &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              Record<
                Uppercase<ContractMethod>,
                LiveTypeFunction<
                  SV,
                  SanitizePathSlashes<`${BasePath}${Path}`>,
                  P,
                  ResBodyMap,
                  ReqBody,
                  ReqQuery,
                  ReqHeaders,
                  ResHeaders,
                  ContractMethod,
                  VersionedApi,
                  Auth
                >
              >
            >
    >;
    sdk: Prettify<
      Sdk &
        Record<
          PrettyCamelCase<
            Name extends string
              ? Name
              : SanitizePathSlashes<`${BasePath}${Path}`>
          >,
          LiveSdkFunction<
            SV,
            P,
            ResBodyMap,
            ReqBody,
            ReqQuery,
            ReqHeaders,
            ResHeaders,
            VersionedApi,
            Auth
          >
        >
    >;
  } {
    // in this case, we know that the first argument is the typedHandler. As a result, we only use defined handlers
    const { contractDetails, handlers } = resolveContractDetailsAndHandlers<
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
      Auth
    >(
      contractDetailsOrMiddlewareOrTypedHandler,
      middlewareOrMiddlewareAndTypedHandler
    );

    validateContractDetails(contractDetails, this.schemaValidator);

    this.routes.push({
      basePath: this.basePath,
      path,
      method,
      contractDetails: contractDetails as PathParamHttpContractDetails<SV>
    });

    const { requestSchema, responseSchemas } = compileRouteSchemas(
      contractDetails,
      this.schemaValidator
    );

    const { middlewares, controllerHandler } = resolveRouteMiddlewares<
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
      basePath: this.basePath,
      path,
      contractDetails,
      requestSchema,
      responseSchemas,
      openTelemetryCollector: this.openTelemetryCollector,
      routerOptions: this.routerOptions,
      postEnrichMiddleware: this.postEnrichMiddleware,
      includeCreateContext: false,
      handlers,
      router: this
    });

    registrationMethod.bind(this.internal)(
      path,
      ...(middlewares as RouterHandler[]),
      this.#parseAndRunControllerHandler(controllerHandler) as RouterHandler
    );

    toRecord(this._fetchMap)[sanitizePathSlashes(`${this.basePath}${path}`)] = {
      ...(this._fetchMap[sanitizePathSlashes(`${this.basePath}${path}`)] ?? {}),
      [method.toUpperCase()]: contractDetails.versions
        ? Object.fromEntries(
            Object.keys(contractDetails.versions).map((version) => [
              version,
              this.#localParamRequest(
                middlewares as RouterHandler[],
                controllerHandler as RouterHandler,
                contractDetails as PathParamHttpContractDetails<SV>,
                requestSchema,
                responseSchemas,
                version
              )
            ])
          )
        : this.#localParamRequest(
            middlewares as RouterHandler[],
            controllerHandler as RouterHandler,
            contractDetails as PathParamHttpContractDetails<SV>,
            requestSchema,
            responseSchemas
          )
    };

    const contractDetailsName = contractDetails.name;
    if (contractDetailsName) {
      toRecord(this.sdk)[toPrettyCamelCase(contractDetailsName)] =
        contractDetails.versions
          ? Object.fromEntries(
              Object.keys(contractDetails.versions).map((version) => [
                version,
                (req: {
                  params?: Record<string, unknown>;
                  query?: Record<string, unknown>;
                  headers?: Record<string, unknown>;
                  body?: Record<string, unknown>;
                }) =>
                  this.#localParamRequest(
                    middlewares as RouterHandler[],
                    controllerHandler as RouterHandler,
                    contractDetails as PathParamHttpContractDetails<SV>,
                    requestSchema,
                    responseSchemas,
                    version
                  )(`${this.basePath}${path}`, req)
              ])
            )
          : (req: {
              params?: Record<string, unknown>;
              query?: Record<string, unknown>;
              headers?: Record<string, unknown>;
              body?: Record<string, unknown>;
            }) =>
              this.#localParamRequest(
                middlewares as RouterHandler[],
                controllerHandler as RouterHandler,
                contractDetails as PathParamHttpContractDetails<SV>,
                requestSchema,
                responseSchemas
              )(`${this.basePath}${path}`, req);
    }
    return this as this & {
      _fetchMap: FetchMap extends Record<
        SanitizePathSlashes<`${BasePath}${Path}`>,
        unknown
      >
        ? FetchMap &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              FetchMap[SanitizePathSlashes<`${BasePath}${Path}`>] &
                Record<
                  Uppercase<ContractMethod>,
                  LiveTypeFunction<
                    SV,
                    SanitizePathSlashes<`${BasePath}${Path}`>,
                    P,
                    ResBodyMap,
                    ReqBody,
                    ReqQuery,
                    ReqHeaders,
                    ResHeaders,
                    ContractMethod,
                    VersionedApi,
                    Auth
                  >
                >
            >
        : FetchMap &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              Record<
                Uppercase<ContractMethod>,
                LiveTypeFunction<
                  SV,
                  SanitizePathSlashes<`${BasePath}${Path}`>,
                  P,
                  ResBodyMap,
                  ReqBody,
                  ReqQuery,
                  ReqHeaders,
                  ResHeaders,
                  ContractMethod,
                  VersionedApi,
                  Auth
                >
              >
            >;
      sdk: Sdk &
        Record<
          PrettyCamelCase<
            Name extends string
              ? Name
              : SanitizePathSlashes<`${BasePath}${Path}`>
          >,
          LiveSdkFunction<
            SV,
            P,
            ResBodyMap,
            ReqBody,
            ReqQuery,
            ReqHeaders,
            ResHeaders,
            VersionedApi,
            Auth
          >
        >;
    };
  }

  #extractHandlers<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    ArrayReturnType,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    handlers: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>
      | RouterHandler
    )[],
    processMiddleware?: (handler: unknown) => RouterHandler | Internal
  ) {
    const last = handlers.pop();
    let finalHandlers = last ? [last] : [];
    if (
      isTypedHandler<
        SV,
        Name,
        'middleware',
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
        Auth
      >(last)
    ) {
      finalHandlers = last.handlers;
    }

    handlers.forEach((handler) => {
      if (
        isTypedHandler<
          SV,
          Name,
          'middleware',
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
          Auth
        >(handler)
      ) {
        throw new Error(
          'Only the last argument supplied to this function can be a typed handler. Please use only middleware.'
        );
      }
    });

    const middleware = processMiddleware
      ? handlers.map(processMiddleware)
      : (handlers as ArrayReturnType[]);

    return [...middleware, ...finalHandlers] as ArrayReturnType[];
  }

  #extractMiddlewareFromEnrichedTypedHandlerArray<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    handlers: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
    )[]
  ) {
    return this.#extractHandlers<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      RouterHandler,
      VersionedApi,
      Auth
    >(handlers);
  }

  #extractNestableMiddlewareFromEnrichedTypedHandlerArray<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    handlers: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>
    )[]
  ) {
    return this.#extractHandlers<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      RouterHandler | Internal,
      VersionedApi,
      Auth
    >(handlers, (handler) => {
      if (
        isForklaunchExpressLikeRouter<
          SV,
          Path,
          RouterHandler,
          Internal,
          BaseRequest,
          BaseResponse,
          NextFunction,
          FetchMap,
          Sdk
        >(handler)
      ) {
        this.addRouterToSdk(handler);
        return handler.internal;
      }
      return handler as RouterHandler;
    });
  }

  #processTypedHandlerOrMiddleware<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    handler:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
      | undefined,
    middleware: RouterHandler[]
  ) {
    if (
      isTypedHandler<
        SV,
        Name,
        'middleware',
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
        Auth
      >(handler)
    ) {
      middleware.push(...(handler.handlers as RouterHandler[]));
    } else if (isExpressLikeSchemaHandler(handler)) {
      middleware.push(handler as RouterHandler);
    }
  }

  #extractMiddlewareAsRouterHandlers<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    contractDetailsOrMiddlewareOrTypedHandler:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
      | undefined,
    middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
    )[]
  ) {
    const middleware: RouterHandler[] = [];

    this.#processTypedHandlerOrMiddleware<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(contractDetailsOrMiddlewareOrTypedHandler, middleware);

    middleware.push(
      ...this.#extractMiddlewareFromEnrichedTypedHandlerArray<
        Name,
        Path,
        P,
        ResBodyMap,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        ResHeaders,
        LocalsObj,
        VersionedApi,
        Auth
      >(middlewareOrMiddlewareWithTypedHandler)
    );

    return middleware;
  }

  #extractNestableMiddlewareAsRouterHandlers<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    contractDetailsOrMiddlewareOrTypedHandler:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>
      | undefined,
    middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>
    )[]
  ) {
    const middleware: (RouterHandler | Internal)[] = [];

    this.#processTypedHandlerOrMiddleware<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      contractDetailsOrMiddlewareOrTypedHandler as ContractDetailsOrMiddlewareOrTypedHandler<
        SV,
        Name,
        'middleware',
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
      middleware as RouterHandler[]
    );

    if (
      isForklaunchExpressLikeRouter<
        SV,
        Path,
        RouterHandler,
        Internal,
        BaseRequest,
        BaseResponse,
        NextFunction,
        FetchMap,
        Sdk
      >(contractDetailsOrMiddlewareOrTypedHandler)
    ) {
      this.addRouterToSdk(contractDetailsOrMiddlewareOrTypedHandler);
      middleware.push(contractDetailsOrMiddlewareOrTypedHandler.internal);
    }

    middleware.push(
      ...this.#extractNestableMiddlewareFromEnrichedTypedHandlerArray<
        Name,
        Path,
        P,
        ResBodyMap,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        ResHeaders,
        LocalsObj,
        VersionedApi,
        Auth
      >(middlewareOrMiddlewareWithTypedHandler)
    );

    return middleware;
  }

  registerMiddlewareHandler<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    registrationMethod: PathOrMiddlewareBasedHandler<RouterHandler>,
    pathOrContractDetailsOrMiddlewareOrTypedHandler:
      | Path
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler,
    contractDetailsOrMiddlewareOrTypedHandler?:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler,
    ...middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
    )[]
  ): this {
    const middleware: RouterHandler[] = [];

    if (typeof pathOrContractDetailsOrMiddlewareOrTypedHandler === 'string') {
      middleware.push(
        ...this.#extractMiddlewareAsRouterHandlers<
          Name,
          Path,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Auth
        >(
          contractDetailsOrMiddlewareOrTypedHandler,
          middlewareOrMiddlewareWithTypedHandler
        )
      );
      const path = pathOrContractDetailsOrMiddlewareOrTypedHandler;
      registrationMethod.bind(this.internal)(path, ...middleware);
    } else {
      middleware.push(
        ...this.#extractMiddlewareAsRouterHandlers<
          Name,
          Path,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Auth
        >(
          pathOrContractDetailsOrMiddlewareOrTypedHandler,
          (
            (isExpressLikeSchemaHandler<
              SV,
              'middleware',
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
              NextFunction
            >(contractDetailsOrMiddlewareOrTypedHandler) ||
            isTypedHandler<
              SV,
              Name,
              'middleware',
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
              Auth
            >(contractDetailsOrMiddlewareOrTypedHandler)
              ? [contractDetailsOrMiddlewareOrTypedHandler]
              : []) as (
              | MiddlewareOrMiddlewareWithTypedHandler<
                  SV,
                  Name,
                  'middleware',
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
                >
              | RouterHandler
            )[]
          ).concat(middlewareOrMiddlewareWithTypedHandler)
        )
      );
      registrationMethod.bind(this.internal)(...middleware);
    }
    return this;
  }

  private addRouterToSdk(
    router: ConstrainedForklaunchRouter<SV, RouterHandler>
  ) {
    Object.entries(router._fetchMap).map(
      ([key, value]) =>
        (toRecord(this._fetchMap)[
          sanitizePathSlashes(`${this.basePath}${key}`)
        ] = value)
    );

    const existingSdk =
      this.sdk[router.sdkName ?? toPrettyCamelCase(router.basePath)];
    toRecord(this.sdk)[router.sdkName ?? toPrettyCamelCase(router.basePath)] = {
      ...(typeof existingSdk === 'object' ? existingSdk : {}),
      ...router.sdk
    };
  }

  registerNestableMiddlewareHandler<
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    const SessionSchema extends SessionObject<SV>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    registrationMethod: NestableRouterBasedHandler<RouterHandler, Internal>,
    pathOrContractDetailsOrMiddlewareOrTypedHandler:
      | Path
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>,
    contractDetailsOrMiddlewareOrTypedHandler?:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>,
    ...middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | ConstrainedForklaunchRouter<SV, RouterHandler>
    )[]
  ): this {
    const middleware: (RouterHandler | Internal)[] = [];
    let path: `/${string}` | undefined;

    if (typeof pathOrContractDetailsOrMiddlewareOrTypedHandler === 'string') {
      middleware.push(
        ...this.#extractNestableMiddlewareAsRouterHandlers<
          Name,
          Path,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Auth
        >(
          contractDetailsOrMiddlewareOrTypedHandler,
          middlewareOrMiddlewareWithTypedHandler
        )
      );
      path = pathOrContractDetailsOrMiddlewareOrTypedHandler;
    } else {
      if (
        isConstrainedForklaunchRouter<SV, RouterHandler>(
          pathOrContractDetailsOrMiddlewareOrTypedHandler
        )
      ) {
        const router = pathOrContractDetailsOrMiddlewareOrTypedHandler;

        this.addRouterToSdk(router);
        path = router.basePath;
      }
      middleware.push(
        ...this.#extractNestableMiddlewareAsRouterHandlers<
          Name,
          Path,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Auth
        >(
          pathOrContractDetailsOrMiddlewareOrTypedHandler,
          (isExpressLikeSchemaHandler<
            SV,
            'middleware',
            P,
            ResBodyMap,
            ReqBody,
            ReqQuery,
            ReqHeaders,
            ResHeaders,
            LocalsObj,
            VersionedApi,
            SessionObject<SV> extends SessionSchema
              ? RouterSession
              : SessionSchema,
            BaseRequest,
            BaseResponse,
            NextFunction
          >(contractDetailsOrMiddlewareOrTypedHandler) ||
          isTypedHandler<
            SV,
            Name,
            'middleware',
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
            Auth
          >(contractDetailsOrMiddlewareOrTypedHandler) ||
          isConstrainedForklaunchRouter<SV, RouterHandler>(
            contractDetailsOrMiddlewareOrTypedHandler
          )
            ? [contractDetailsOrMiddlewareOrTypedHandler]
            : []
          ).concat(middlewareOrMiddlewareWithTypedHandler)
        )
      );
    }

    if (!path) {
      path = middleware.filter((m) =>
        isForklaunchExpressLikeRouter<
          SV,
          Path,
          RouterHandler,
          Internal,
          BaseRequest,
          BaseResponse,
          NextFunction,
          FetchMap,
          Sdk
        >(m)
      )[0]?.basePath;
    }

    if (path) {
      registrationMethod.bind(this.internal)(path, ...middleware);
    } else {
      registrationMethod.bind(this.internal)(...middleware);
    }
    return this;
  }

  private addRouterOptions(
    router: ConstrainedForklaunchRouter<SV, RouterHandler>
  ) {
    router.routerOptions = {
      ...(this.routerOptions ?? {}),
      ...(router.routerOptions ?? {})
    } as typeof this.routerOptions;
    router.routers.forEach((subRouter) => {
      this.addRouterOptions(
        subRouter as ConstrainedForklaunchRouter<SV, RouterHandler>
      );
    });
  }

  use: TypedNestableMiddlewareDefinition<
    this,
    RouterHandler,
    Internal,
    SV,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    Router extends ConstrainedForklaunchRouter<SV, RouterHandler>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    SessionSchema extends SessionObject<SV>,
    ResolvedSchema extends SessionObject<SV> extends SessionSchema
      ? RouterSession
      : SessionSchema,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    pathOrContractDetailsOrMiddlewareOrTypedHandler:
      | Path
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | Router,
    contractDetailsOrMiddlewareOrTypedHandler?:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | Router,
    ...middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | Router
    )[]
  ) => {
    [
      pathOrContractDetailsOrMiddlewareOrTypedHandler,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    ].forEach((arg) => {
      if (isForklaunchRouter<SV>(arg)) {
        this.routers.push(arg);
        this.addRouterOptions(
          arg as ConstrainedForklaunchRouter<SV, RouterHandler>
        );
      }
    });

    return this.registerNestableMiddlewareHandler<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSchema,
      Auth
    >(
      this.internal.use,
      pathOrContractDetailsOrMiddlewareOrTypedHandler,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    ) as this & {
      _fetchMap: FetchMap & {
        [Key in keyof Router['_fetchMap'] as Key extends string
          ? SanitizePathSlashes<`${BasePath}${Key}`>
          : never]: Router['_fetchMap'][Key];
      };
      sdk: Sdk & {
        [Key in PrettyCamelCase<
          Router extends { sdkName?: string; basePath: string }
            ? string extends Router['sdkName']
              ? Router['basePath']
              : Router['sdkName']
            : never
        >]: Router['sdk'];
      };
    };
  };

  all: TypedMiddlewareDefinition<
    this,
    SV,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    RouterHandler
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    pathOrContractDetailsOrMiddlewareOrTypedHandler:
      | Path
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler,
    contractDetailsOrMiddlewareOrTypedHandler?:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler,
    ...middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
    )[]
  ) => {
    return this.registerMiddlewareHandler<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      this.internal.all,
      pathOrContractDetailsOrMiddlewareOrTypedHandler,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  connect: TypedMiddlewareDefinition<
    this,
    SV,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    RouterHandler
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'middleware'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    pathOrContractDetailsOrMiddlewareOrTypedHandler:
      | Path
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler,
    contractDetailsOrMiddlewareOrTypedHandler?:
      | ContractDetailsOrMiddlewareOrTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler,
    ...middlewareOrMiddlewareWithTypedHandler: (
      | MiddlewareOrMiddlewareWithTypedHandler<
          SV,
          Name,
          'middleware',
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
        >
      | RouterHandler
    )[]
  ) => {
    return this.registerMiddlewareHandler<
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      this.internal.connect,
      pathOrContractDetailsOrMiddlewareOrTypedHandler,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a GET route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {PathParamHttpContractDetails<SV, P, ResBodyMap, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  get: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'get',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'get'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'get',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'get',
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
  ) => {
    return this.registerRoute<
      Name,
      'get',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'get',
      path,
      this.internal.get,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a POST route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {HttpContractDetails<SV, P, ResBodyMap, ReqBody, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Expxwress router.
   */
  post: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'post',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'post'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'post',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'post',
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
  ) => {
    return this.registerRoute<
      Name,
      'post',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'post',
      path,
      this.internal.post,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a PUT route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {HttpContractDetails<SV, P, ResBodyMap, ReqBody, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  put: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'put',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'put'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'put',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'put',
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
  ) => {
    return this.registerRoute<
      Name,
      'put',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'put',
      path,
      this.internal.put,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a PATCH route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {HttpContractDetails<SV, P, ResBodyMap, ReqBody, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  patch: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'patch',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'patch'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'patch',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'patch',
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
  ) => {
    return this.registerRoute<
      Name,
      'patch',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'patch',
      path,
      this.internal.patch,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a DELETE route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {PathParamHttpContractDetails<SV, P, ResBodyMap, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  delete: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'delete',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'delete'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'delete',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'delete',
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
  ) => {
    return this.registerRoute<
      Name,
      'delete',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'delete',
      path,
      this.internal.delete,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a OPTIONS route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {PathParamHttpContractDetails<SV, P, ResBodyMap, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  options: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'options',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'options'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'options',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'options',
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
  ) => {
    return this.registerRoute<
      Name,
      'options',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'options',
      path,
      this.internal.options,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a HEAD route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {PathParamHttpContractDetails<SV, P, ResBodyMap, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  head: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'head',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'head'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'head',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'head',
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
  ) => {
    return this.registerRoute<
      Name,
      'head',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'head',
      path,
      this.internal.head,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  /**
   * Registers a TRACE route with the specified contract details and handler handlers.
   *
   * @template P - The type of request parameters.
   * @template ResBodyMap - The type of response body.
   * @template ReqBody - The type of request body.
   * @template ReqQuery - The type of request query.
   * @template LocalsObj - The type of local variables.
   * @param {string} path - The path for the route.
   * @param {PathParamHttpContractDetails<SV, P, ResBodyMap, ReqQuery>} contractDetails - The contract details.
   * @param {...SchemaHandler<SV, P, ResBodyMap, ReqBody, ReqQuery, LocalsObj>[]} handlers - The handler handlers.
   * @returns {ExpressRouter} - The Express router.
   */
  trace: LiveTypeRouteDefinition<
    SV,
    BasePath,
    'trace',
    RouterHandler,
    Internal,
    RouterSession,
    BaseRequest,
    BaseResponse,
    NextFunction,
    this
  > = <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    VersionedApi extends VersionSchema<SV, 'trace'>,
    Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: Path,
    contractDetailsOrMiddlewareOrTypedHandler: ContractDetailsOrMiddlewareOrTypedHandler<
      SV,
      Name,
      'trace',
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
    ...middlewareOrMiddlewareWithTypedHandler: MiddlewareOrMiddlewareWithTypedHandler<
      SV,
      Name,
      'trace',
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
  ) => {
    return this.registerRoute<
      Name,
      'trace',
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      Auth
    >(
      'trace',
      path,
      this.internal.trace,
      contractDetailsOrMiddlewareOrTypedHandler,
      ...middlewareOrMiddlewareWithTypedHandler
    );
  };

  insertIntoRouterSdkPaths({
    sdkPath,
    path,
    method,
    name
  }: {
    sdkPath: string;
    name: string;
    path: string;
    method: string;
  }) {
    const routePath = [method, path].join('.');
    for (const route of this.routes) {
      if (
        route.path === path &&
        route.method === method &&
        route.contractDetails.name === name
      ) {
        this.sdkPaths[routePath] = sdkPath;
      }
    }

    for (const router of this.routers) {
      router.insertIntoRouterSdkPaths?.({
        sdkPath,
        path,
        method,
        name
      });
    }
  }

  private unpackSdks(
    sdks: SdkHandlerObject<SV>,
    path: string[],
    routerUniquenessCache: Set<number>
  ) {
    Object.entries(sdks).forEach(([key, maybeHandler]) => {
      if (isSdkHandler(maybeHandler)) {
        const cacheKey = hashString(safeStringify(maybeHandler));
        if (routerUniquenessCache.has(cacheKey)) {
          throw new Error(`SDK handler ${key} is already registered`);
        }
        routerUniquenessCache.add(cacheKey);
        if (!maybeHandler._method || !maybeHandler._path) {
          throw new Error(`SDK handler ${key} is missing method or path`);
        }
        this.insertIntoRouterSdkPaths({
          sdkPath: [...path, key].join('.'),
          path: maybeHandler._path,
          method: maybeHandler._method,
          name: maybeHandler.contractDetails.name
        });
        routerUniquenessCache.add(cacheKey);
      } else {
        this.unpackSdks(maybeHandler, [...path, key], routerUniquenessCache);
      }
    });
  }

  registerSdks(sdks: SdkHandlerObject<SV>) {
    this.unpackSdks(sdks, [], new Set());
  }

  protected cloneInternals(clone: this): void {
    clone.routers = [...this.routers];
    clone.routes = [...this.routes];
    clone._fetchMap = { ...this._fetchMap };
    clone.sdk = { ...this.sdk };
    clone.sdkPaths = { ...this.sdkPaths };
  }

  clone(): this {
    const clone = new ForklaunchExpressLikeRouter<
      SV,
      BasePath,
      RouterHandler,
      Internal,
      BaseRequest,
      BaseResponse,
      NextFunction,
      RouterSession,
      FetchMap,
      Sdk
    >(
      this.basePath,
      this.schemaValidator,
      this.internal,
      this.postEnrichMiddleware,
      this.openTelemetryCollector,
      this.routerOptions
    ) as this;

    this.cloneInternals(clone);

    return clone;
  }
}
