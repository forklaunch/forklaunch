import {
  Prettify,
  PrettyCamelCase,
  SanitizePathSlashes
} from '@forklaunch/common';
import { AnySchemaValidator } from '@forklaunch/validator';
import { ExpressLikeRouter } from '../interfaces/expressLikeRouter.interface';
import {
  ExpressLikeSchemaHandler,
  LiveSdkFunction,
  LiveTypeFunction,
  PathMatch
} from './apiDefinition.types';
import {
  Body,
  ContractDetails,
  HeadersObject,
  Method,
  ParamsObject,
  QueryObject,
  ResolvedSessionObject,
  ResponsesObject,
  SchemaAuthMethods,
  SessionObject,
  VersionSchema
} from './contractDetails.types';
import { ConstrainedForklaunchRouter } from './router.types';
import { TypedHandler } from './typedHandler.types';

export interface LiveTypeRouteDefinition<
  SV extends AnySchemaValidator,
  BasePath extends `/${string}`,
  ContractMethod extends Method,
  RouterHandler,
  Internal extends ExpressLikeRouter<RouterHandler, Internal>,
  Session extends SessionObject<SV>,
  BaseRequest,
  BaseResponse,
  NextFunction,
  ChainableRouter extends {
    _fetchMap: object;
    sdk: object;
  }
> {
  <
    Name extends string,
    Path extends `/${string}`,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    const VersionedApi extends VersionSchema<SV, ContractMethod>,
    const Auth extends SchemaAuthMethods<
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
    ...middleware: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >[]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] extends Record<
        SanitizePathSlashes<`${BasePath}${Path}`>,
        unknown
      >
        ? ChainableRouter['_fetchMap'] &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              ChainableRouter['_fetchMap'][SanitizePathSlashes<`${BasePath}${Path}`>] &
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
        : ChainableRouter['_fetchMap'] &
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
      ChainableRouter['sdk'] &
        Record<
          PrettyCamelCase<Name>,
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
  };

  <
    Name extends string,
    Path extends `/${string}`,
    SuppliedPath extends Path,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    const VersionedApi extends VersionSchema<SV, ContractMethod>,
    const Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: PathMatch<SuppliedPath, Path>,
    middleware: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >,
    ...middlewareAndTypedHandler: [
      ...ExpressLikeSchemaHandler<
        SV,
        P,
        ResBodyMap,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        ResHeaders,
        LocalsObj,
        VersionedApi,
        ResolvedSessionObject<SV, Auth, Session>,
        BaseRequest,
        BaseResponse,
        NextFunction
      >[],
      TypedHandler<
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
      >
    ]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] extends Record<
        SanitizePathSlashes<`${BasePath}${Path}`>,
        unknown
      >
        ? ChainableRouter['_fetchMap'] &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              ChainableRouter['_fetchMap'][SanitizePathSlashes<`${BasePath}${Path}`>] &
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
        : ChainableRouter['_fetchMap'] &
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
      ChainableRouter['sdk'] &
        Record<
          PrettyCamelCase<Name>,
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
  };

  <
    Name extends string,
    Path extends `/${string}`,
    SuppliedPath extends Path,
    P extends ParamsObject<SV>,
    ResBodyMap extends ResponsesObject<SV>,
    ReqBody extends Body<SV>,
    ReqQuery extends QueryObject<SV>,
    ReqHeaders extends HeadersObject<SV>,
    ResHeaders extends HeadersObject<SV>,
    LocalsObj extends Record<string, unknown>,
    const VersionedApi extends VersionSchema<SV, ContractMethod>,
    const Auth extends SchemaAuthMethods<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedApi,
      BaseRequest
    >
  >(
    path: PathMatch<SuppliedPath, Path>,
    typedHandler: TypedHandler<
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
    >
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] extends Record<
        SanitizePathSlashes<`${BasePath}${Path}`>,
        unknown
      >
        ? ChainableRouter['_fetchMap'] &
            Record<
              SanitizePathSlashes<`${BasePath}${Path}`>,
              ChainableRouter['_fetchMap'][SanitizePathSlashes<`${BasePath}${Path}`>] &
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
        : ChainableRouter['_fetchMap'] &
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
      ChainableRouter['sdk'] &
        Record<
          PrettyCamelCase<Name>,
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
  };
}

export interface TypedMiddlewareDefinition<
  ChainableRouter,
  SV extends AnySchemaValidator,
  Session extends SessionObject<SV>,
  BaseRequest,
  BaseResponse,
  NextFunction,
  RouterHandler
> {
  <
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
    contractDetails: ContractDetails<
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
      VersionedApi,
      BaseRequest,
      Auth
    >,
    middleware: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >,
    ...middlewares: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >[]
  ): ChainableRouter;

  <
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
    SessionSchema extends SessionObject<SV>,
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
    contractDetails: ContractDetails<
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
      VersionedApi,
      BaseRequest,
      Auth
    >,
    middleware: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >,
    ...middlewares: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      // ResolvedSession,
      SessionSchema,
      BaseRequest,
      BaseResponse,
      NextFunction
    >[]
  ): ChainableRouter;

  (
    middleware: RouterHandler,
    ...otherMiddleware: RouterHandler[]
  ): ChainableRouter;

  <
    Name extends string,
    Path extends `/${string}`,
    SuppliedPath extends Path,
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
    path: PathMatch<SuppliedPath, Path>,
    middleware: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >,
    ...middlewareAndTypedHandler: [
      ...ExpressLikeSchemaHandler<
        SV,
        P,
        ResBodyMap,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        ResHeaders,
        LocalsObj,
        VersionedApi,
        ResolvedSessionObject<SV, Auth, Session>,
        BaseRequest,
        BaseResponse,
        NextFunction
      >[],
      TypedHandler<
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
      >
    ]
  ): ChainableRouter;

  <
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
    middleware: ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, Session>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >,
    ...middlewareAndTypedHandler: [
      ...ExpressLikeSchemaHandler<
        SV,
        P,
        ResBodyMap,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        ResHeaders,
        LocalsObj,
        VersionedApi,
        ResolvedSessionObject<SV, Auth, Session>,
        BaseRequest,
        BaseResponse,
        NextFunction
      >[],
      TypedHandler<
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
      >
    ]
  ): ChainableRouter;

  // TypedHandler overloads moved to the end
  <
    Name extends string,
    Path extends `/${string}`,
    SuppliedPath extends Path,
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
    path: PathMatch<SuppliedPath, Path>,
    typedHandler: TypedHandler<
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
    >
  ): ChainableRouter;

  <
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
    typedHandler: TypedHandler<
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
    >
  ): ChainableRouter;
}

export interface TypedNestableMiddlewareDefinition<
  ChainableRouter extends {
    _fetchMap: object;
    basePath: `/${string}`;
    sdk: Record<string, unknown>;
  },
  RouterHandler,
  Internal extends ExpressLikeRouter<RouterHandler, Internal>,
  SV extends AnySchemaValidator,
  Session extends SessionObject<SV>,
  BaseRequest,
  BaseResponse,
  NextFunction
> extends TypedMiddlewareDefinition<
  ChainableRouter,
  SV,
  Session,
  BaseRequest,
  BaseResponse,
  NextFunction,
  RouterHandler
> {
  (
    middleware: RouterHandler,
    ...otherMiddleware: RouterHandler[]
  ): ChainableRouter;

  <Router extends ConstrainedForklaunchRouter<SV, RouterHandler>>(
    router: Router
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <Router extends ConstrainedForklaunchRouter<SV, RouterHandler>>(
    middlewareOrRouter: RouterHandler | Router,
    ...otherMiddlewareOrRouters: [...(RouterHandler | Router)[]]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <Router extends ConstrainedForklaunchRouter<SV, RouterHandler>>(
    path: Router['basePath'],
    router: Router
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <
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
      'middleware',
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
    middleware:
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router,
    ...middlewares: (
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router
    )[]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <
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
    middleware:
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router,
    ...middlewareAndTypedHandler: [
      ...(
        | ExpressLikeSchemaHandler<
            SV,
            P,
            ResBodyMap,
            ReqBody,
            ReqQuery,
            ReqHeaders,
            ResHeaders,
            LocalsObj,
            VersionedApi,
            Session,
            BaseRequest,
            BaseResponse,
            NextFunction
          >
        | Router
      )[],
      TypedHandler<
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
      >
    ]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <
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
    path: `/${string}` extends Router['basePath']
      ? Path
      : PathMatch<Path, Router['basePath']>,
    contractDetails: ContractDetails<
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
      VersionedApi,
      BaseRequest,
      Auth
    >,
    middleware:
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router,
    ...middlewares: (
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router
    )[]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <
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
      'middleware',
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
    middleware:
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router,
    ...middlewares: (
      | ExpressLikeSchemaHandler<
          SV,
          P,
          ResBodyMap,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          ResHeaders,
          LocalsObj,
          VersionedApi,
          Session,
          BaseRequest,
          BaseResponse,
          NextFunction
        >
      | Router
    )[]
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <
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
    path: `/${string}` extends Router['basePath']
      ? Path
      : PathMatch<Path, Router['basePath']>,
    typedHandler: TypedHandler<
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
    >
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };

  <
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
    typedHandler: TypedHandler<
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
    >
  ): ChainableRouter & {
    _fetchMap: Prettify<
      ChainableRouter['_fetchMap'] & {
        [
          Key in keyof Router['_fetchMap'] as Key extends string
            ? SanitizePathSlashes<`${ChainableRouter['basePath']}${Key}`>
            : never
        ]: Router['_fetchMap'][Key];
      }
    >;
    sdk: Prettify<
      ChainableRouter['sdk'] & {
        [
          K in PrettyCamelCase<
            Router['sdkName'] extends string
              ? Router['sdkName']
              : Router['basePath']
          >
        ]: Prettify<Router['sdk']>;
      }
    >;
  };
}

export type ContractDetailsOrMiddlewareOrTypedHandler<
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
  BaseRequest,
  BaseResponse,
  NextFunction,
  BaseSession extends SessionObject<SV>,
  Auth extends SchemaAuthMethods<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >
> =
  | ContractDetails<
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
    >
  | ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, BaseSession>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >
  | TypedHandler<
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
    >;

export type MiddlewareOrMiddlewareWithTypedHandler<
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
  BaseRequest,
  BaseResponse,
  NextFunction,
  BaseSession extends SessionObject<SV>,
  Auth extends SchemaAuthMethods<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >
> =
  | ExpressLikeSchemaHandler<
      SV,
      P,
      ResBodyMap,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      ResHeaders,
      LocalsObj,
      VersionedApi,
      ResolvedSessionObject<SV, Auth, BaseSession>,
      BaseRequest,
      BaseResponse,
      NextFunction
    >
  | TypedHandler<
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
    >;
