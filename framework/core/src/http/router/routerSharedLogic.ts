import { isRecord } from '@forklaunch/common';
import { AnySchemaValidator, SchemaValidator } from '@forklaunch/validator';
import { hasPermissionChecks } from '../guards/hasPermissionChecks';
import { hasRoleChecks } from '../guards/hasRoleChecks';
import { hasScopeChecks } from '../guards/hasScopeChecks';
import { hasVersionedSchema } from '../guards/hasVersionedSchema';
import { isExpressLikeSchemaHandler } from '../guards/isExpressLikeSchemaHandler';
import { isHmacMethod } from '../guards/isHmacMethod';
import { isHttpContractDetails } from '../guards/isHttpContractDetails';
import { isPathParamHttpContractDetails } from '../guards/isPathParamContractDetails';
import { isTypedHandler } from '../guards/isTypedHandler';
import { parseRequestAuth } from '../middleware/request/auth.middleware';
import { createContext } from '../middleware/request/createContext.middleware';
import { enrichDetails } from '../middleware/request/enrichDetails.middleware';
import { parse } from '../middleware/request/parse.middleware';
import { OpenTelemetryCollector } from '../telemetry/openTelemetryCollector';
import { ExpressLikeSchemaHandler } from '../types/apiDefinition.types';
import {
  Body,
  ContractDetails,
  HeadersObject,
  HttpMethod,
  Method,
  ParamsDictionary,
  ParamsObject,
  PathParamHttpContractDetails,
  PathParamMethod,
  QueryObject,
  ResponseCompiledSchema,
  ResponsesObject,
  SchemaAuthMethods,
  SessionObject,
  VersionSchema
} from '../types/contractDetails.types';
import { ExpressLikeRouterOptions } from '../types/expressLikeOptions';
import {
  ContractDetailsOrMiddlewareOrTypedHandler,
  MiddlewareOrMiddlewareWithTypedHandler
} from '../types/expressLikeRouter.types';
import { MetricsDefinition } from '../types/openTelemetryCollector.types';
import {
  discriminateBody,
  discriminateResponseBodies
} from './discriminateBody';

export function resolveContractDetailsAndHandlers<
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
  >
>(
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
  >[]
) {
  let contractDetails: ContractDetails<
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
    VersionedApi,
    BaseRequest,
    Auth
  >;
  // eslint-disable-next-line no-useless-assignment
  let handlers: ExpressLikeSchemaHandler<
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
  >[] = [];

  if (
    isTypedHandler<
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
      Auth
    >(contractDetailsOrMiddlewareOrTypedHandler)
  ) {
    contractDetails = contractDetailsOrMiddlewareOrTypedHandler.contractDetails;
    handlers =
      contractDetailsOrMiddlewareOrTypedHandler.handlers as ExpressLikeSchemaHandler<
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
      >[];
  } else {
    const maybeTypedHandler =
      middlewareOrMiddlewareAndTypedHandler[
        middlewareOrMiddlewareAndTypedHandler.length - 1
      ];
    if (
      isTypedHandler<
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
        Auth
      >(maybeTypedHandler)
    ) {
      contractDetails = maybeTypedHandler.contractDetails;
      const typedHandlerHandlers = maybeTypedHandler.handlers;
      const finalHandlers: MiddlewareOrMiddlewareWithTypedHandler<
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
      >[] = [];

      if (
        isExpressLikeSchemaHandler<
          SV,
          ContractMethod,
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
        >(contractDetailsOrMiddlewareOrTypedHandler)
      ) {
        finalHandlers.push(contractDetailsOrMiddlewareOrTypedHandler);
      }
      finalHandlers.push(...middlewareOrMiddlewareAndTypedHandler.slice(0, -1));
      finalHandlers.push(
        ...(typedHandlerHandlers as ExpressLikeSchemaHandler<
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
        >[] as MiddlewareOrMiddlewareWithTypedHandler<
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
        >[])
      );

      handlers = (finalHandlers as unknown[]).filter((handler) =>
        isExpressLikeSchemaHandler<
          SV,
          ContractMethod,
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
        >(handler)
      ) as ExpressLikeSchemaHandler<
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
      >[];
    } else {
      if (
        isExpressLikeSchemaHandler(contractDetailsOrMiddlewareOrTypedHandler) ||
        isTypedHandler<
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
          Auth
        >(contractDetailsOrMiddlewareOrTypedHandler)
      ) {
        throw new Error('Contract details are not defined');
      }
      contractDetails =
        contractDetailsOrMiddlewareOrTypedHandler as ContractDetails<
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
          VersionedApi,
          BaseRequest,
          Auth
        >;

      handlers = (middlewareOrMiddlewareAndTypedHandler as unknown[]).filter(
        (handler) =>
          isExpressLikeSchemaHandler<
            SV,
            ContractMethod,
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
          >(handler)
      ) as ExpressLikeSchemaHandler<
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
      >[];
    }
  }

  return { contractDetails, handlers };
}

export function validateContractDetails<
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
  VersionedApi extends VersionSchema<SV, ContractMethod>,
  BaseRequest,
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
  contractDetails: ContractDetails<
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
    VersionedApi,
    BaseRequest,
    Auth
  >,
  schemaValidator: SV
) {
  if (
    !isHttpContractDetails<
      SV,
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      VersionedApi extends VersionSchema<SV, HttpMethod> ? VersionedApi : never,
      BaseRequest,
      Auth
    >(contractDetails) &&
    !isPathParamHttpContractDetails<
      SV,
      Name,
      Path,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      VersionedApi extends VersionSchema<SV, PathParamMethod>
        ? VersionedApi
        : never,
      BaseRequest,
      Auth
    >(contractDetails)
  ) {
    throw new Error('Contract details are malformed for route definition');
  }

  // Validate access field and auth narrowing (runtime safety net for JS users)
  const access = (contractDetails as Record<string, unknown>)['access'] as
    | string
    | undefined;
  const auth = (contractDetails as Record<string, unknown>)['auth'];

  if (access != null) {
    const validAccess = ['public', 'authenticated', 'protected', 'internal'];
    if (!validAccess.includes(access)) {
      throw new Error(
        `Route '${contractDetails.name}': invalid access level '${access}'. ` +
          `Must be one of: ${validAccess.join(', ')}`
      );
    }

    if (access === 'public' && auth != null) {
      throw new Error(
        `Route '${contractDetails.name}': access 'public' cannot have auth configured`
      );
    }

    if (access === 'protected') {
      if (!auth) {
        throw new Error(
          `Route '${contractDetails.name}': access 'protected' requires auth with roles, permissions, or scope`
        );
      }
      if (
        !hasPermissionChecks(auth) &&
        !hasRoleChecks(auth) &&
        !hasScopeChecks(auth)
      ) {
        throw new Error(
          `Route '${contractDetails.name}': access 'protected' requires at least one of ` +
            `allowedRoles, forbiddenRoles, allowedPermissions, forbiddenPermissions, or requiredScope`
        );
      }
    }

    if (access === 'internal') {
      if (!auth || !isHmacMethod(auth)) {
        throw new Error(
          `Route '${contractDetails.name}': access 'internal' requires HMAC auth`
        );
      }
    }
  }

  if (contractDetails.versions) {
    const parserTypes = Object.values(contractDetails.versions).map(
      (version) => discriminateBody(schemaValidator, version.body)?.parserType
    );

    const allParserTypesSame =
      parserTypes.length === 0 ||
      parserTypes.every((pt) => pt === parserTypes[0]);

    if (!allParserTypesSame) {
      throw new Error(
        'All versioned contractDetails must have the same parsing type for body.'
      );
    }
  }
}

export function processContractDetailsIO<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>
>(
  schemaValidator: SV & SchemaValidator,
  contractDetailsIO: {
    requestHeaders?: HeadersObject<SV>;
    responseHeaders?: HeadersObject<SV>;
    query?: QueryObject<SV>;
    body?: Body<SV>;
    responses: ResponsesObject<SV>;
  },
  routeParams?: P
) {
  const responseSchemas = {
    400: schemaValidator.string,
    401: schemaValidator.string,
    403: schemaValidator.string,
    404: schemaValidator.string,
    500: schemaValidator.string,
    ...Object.fromEntries(
      Object.entries(
        discriminateResponseBodies(schemaValidator, contractDetailsIO.responses)
      ).map(([key, value]) => [Number(key), value.schema])
    )
  };

  return {
    requestSchema: schemaValidator.compile(
      schemaValidator.schemify({
        ...(routeParams != null
          ? { params: routeParams as unknown as ParamsDictionary }
          : { params: schemaValidator.unknown as ParamsObject<SV> }),
        ...(contractDetailsIO.requestHeaders != null
          ? { headers: contractDetailsIO.requestHeaders }
          : { headers: schemaValidator.unknown as HeadersObject<SV> }),
        ...(contractDetailsIO.query != null
          ? { query: contractDetailsIO.query }
          : { query: schemaValidator.unknown as QueryObject<SV> }),
        ...(contractDetailsIO.body != null
          ? {
              body: discriminateBody(schemaValidator, contractDetailsIO.body)
                ?.schema
            }
          : { body: schemaValidator.unknown as Body<SV> })
      })
    ),
    responseSchemas: {
      ...(contractDetailsIO.responseHeaders != null
        ? {
            headers: schemaValidator.compile(
              schemaValidator.schemify(contractDetailsIO.responseHeaders)
            )
          }
        : { headers: schemaValidator.unknown as HeadersObject<SV> }),
      responses: Object.fromEntries(
        Object.entries(responseSchemas).map(([key, value]) => [
          key,
          schemaValidator.compile(schemaValidator.schemify(value))
        ])
      )
    }
  };
}

export function compileRouteSchemas<
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
  VersionedApi extends VersionSchema<SV, ContractMethod>,
  BaseRequest,
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
  contractDetails: ContractDetails<
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
    VersionedApi,
    BaseRequest,
    Auth
  >,
  schemaValidator: SV
) {
  const validator = schemaValidator as SV & SchemaValidator;
  let requestSchema: unknown | Record<string, unknown>;
  let responseSchemas:
    | ResponseCompiledSchema
    | Record<string, ResponseCompiledSchema>;

  if (hasVersionedSchema(contractDetails)) {
    requestSchema = {};
    responseSchemas = {};

    Object.entries(contractDetails.versions ?? {}).forEach(
      ([version, versionedContractDetails]) => {
        const {
          requestSchema: versionedRequestSchema,
          responseSchemas: versionedResponseSchemas
        } = processContractDetailsIO(
          validator,
          versionedContractDetails,
          contractDetails.params as unknown as P
        );

        if (isRecord(requestSchema)) {
          requestSchema = {
            ...requestSchema,
            [version]: versionedRequestSchema
          };
        }
        if (isRecord(responseSchemas)) {
          responseSchemas = {
            ...responseSchemas,
            [version]: versionedResponseSchemas
          };
        }
      }
    );
  } else {
    const {
      requestSchema: unversionedRequestSchema,
      responseSchemas: unversionedResponseSchemas
    } = processContractDetailsIO(
      validator,
      {
        ...('params' in contractDetails && contractDetails.params != null
          ? { params: contractDetails.params }
          : { params: validator.unknown as ParamsObject<SV> }),
        ...('requestHeaders' in contractDetails &&
        contractDetails.requestHeaders != null
          ? { requestHeaders: contractDetails.requestHeaders }
          : {
              requestHeaders: validator.unknown as HeadersObject<SV>
            }),
        ...('responseHeaders' in contractDetails &&
        contractDetails.responseHeaders != null
          ? { responseHeaders: contractDetails.responseHeaders }
          : {
              responseHeaders: validator.unknown as HeadersObject<SV>
            }),
        ...('query' in contractDetails && contractDetails.query != null
          ? { query: contractDetails.query }
          : {
              query: validator.unknown as QueryObject<SV>
            }),
        ...('body' in contractDetails && contractDetails.body != null
          ? { body: contractDetails.body }
          : {
              body: validator.unknown as Body<SV>
            }),
        responses:
          'responses' in contractDetails && contractDetails.responses != null
            ? contractDetails.responses
            : (validator.unknown as ResponsesObject<SV>)
      },
      contractDetails.params as unknown as P
    );

    requestSchema = unversionedRequestSchema;
    responseSchemas = unversionedResponseSchemas;
  }

  return { requestSchema, responseSchemas };
}

export function resolveRouteMiddlewares<
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
  basePath: string;
  path: Path;
  contractDetails: ContractDetails<
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
    VersionedApi,
    BaseRequest,
    Auth
  >;
  requestSchema: unknown;
  responseSchemas:
    | ResponseCompiledSchema
    | Record<string, ResponseCompiledSchema>;
  openTelemetryCollector?: OpenTelemetryCollector<MetricsDefinition>;
  routerOptions?: ExpressLikeRouterOptions<SV, RouterSession>;
  postEnrichMiddleware?: RouterHandler[];
  includeCreateContext?: boolean;
  handlers: ExpressLikeSchemaHandler<
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
  >[];
  router?: { routerOptions?: ExpressLikeRouterOptions<SV, RouterSession> };
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
  const handlersCopy = [...params.handlers];
  const controllerHandler = handlersCopy.pop();

  if (typeof controllerHandler !== 'function') {
    throw new Error(
      `Last argument must be a handler, received: ${controllerHandler}`
    );
  }

  const middlewares = [
    ...(params.includeCreateContext !== false ? [createContext] : []),
    enrichDetails<
      SV,
      ContractMethod,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      RouterSession
    >(
      `${params.basePath}${params.path}`,
      params.contractDetails as PathParamHttpContractDetails<SV>,
      params.requestSchema,
      params.responseSchemas,
      params.openTelemetryCollector,
      // Use dynamic lookup from router instance instead of captured params
      () => (params.router ? params.router.routerOptions : params.routerOptions)
    ),
    ...(params.postEnrichMiddleware as unknown[]),
    parse,
    parseRequestAuth<
      SV,
      ContractMethod,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      RouterSession
    >,
    ...handlersCopy
  ] as unknown as RouterHandler[];

  return {
    middlewares,
    controllerHandler: controllerHandler as ExpressLikeSchemaHandler<
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
    >
  };
}
