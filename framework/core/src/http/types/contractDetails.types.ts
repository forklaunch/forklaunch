import {
  StringWithoutSlash,
  TypeSafeFunction,
  UnionToIntersection
} from '@forklaunch/common';
import {
  AnySchemaValidator,
  IdiomaticSchema,
  Schema,
  UnboxedObjectSchema
} from '@forklaunch/validator';
import { JWK, JWTPayload } from 'jose';
import {
  ExpressLikeAuthMapper,
  ExpressLikeSchemaAuthMapper,
  VersionedRequests
} from './apiDefinition.types';

/**
 * Dictionary type for URL parameters.
 */
export type ParamsDictionary = { [key: string]: string };

/**
 * Type representing an object with only string keys.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type StringOnlyObject<SV extends AnySchemaValidator> = Omit<
  UnboxedObjectSchema<SV>,
  number | symbol
>;

/**
 * Type representing an object with only number keys.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type NumberOnlyObject<SV extends AnySchemaValidator> = Omit<
  UnboxedObjectSchema<SV>,
  string | symbol
>;

/**
 * Type representing the body object in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type BodyObject<SV extends AnySchemaValidator> = StringOnlyObject<SV>;

/**
 * Type representing the parameters object in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type ParamsObject<SV extends AnySchemaValidator> = StringOnlyObject<SV>;

/**
 * Type representing the query object in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type QueryObject<SV extends AnySchemaValidator> = StringOnlyObject<SV>;

/**
 * Type representing the headers object in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type HeadersObject<SV extends AnySchemaValidator> = StringOnlyObject<SV>;

export type RawTypedResponseBody<SV extends AnySchemaValidator> =
  | TextBody<SV>
  | JsonBody<SV>
  | FileBody<SV>
  | ServerSentEventBody<SV>
  | UnknownResponseBody<SV>;

type ExclusiveResponseBodyBase<SV extends AnySchemaValidator> = {
  [K in keyof UnionToIntersection<RawTypedResponseBody<SV>>]?: undefined;
};

type ExclusiveSchemaCatchall<SV extends AnySchemaValidator> = {
  [K in keyof SV['_SchemaCatchall'] as string extends K
    ? never
    : number extends K
      ? never
      : symbol extends K
        ? never
        : K]?: undefined;
};

export type TypedResponseBody<SV extends AnySchemaValidator> =
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveResponseBodyBase<SV>)]?: K extends keyof TextBody<SV>
        ? TextBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveResponseBodyBase<SV>)]?: K extends keyof JsonBody<SV>
        ? JsonBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveResponseBodyBase<SV>)]?: K extends keyof FileBody<SV>
        ? FileBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveResponseBodyBase<SV>)]?: K extends keyof ServerSentEventBody<SV>
        ? ServerSentEventBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveResponseBodyBase<SV>)]?: K extends keyof UnknownResponseBody<SV>
        ? UnknownResponseBody<SV>[K]
        : undefined;
    };

export type ResponseBody<SV extends AnySchemaValidator> =
  | TypedResponseBody<SV>
  | (ExclusiveResponseBodyBase<SV> & SV['_ValidSchemaObject'])
  | (ExclusiveResponseBodyBase<SV> & UnboxedObjectSchema<SV>)
  | (ExclusiveResponseBodyBase<SV> & SV['string'])
  | (ExclusiveResponseBodyBase<SV> & SV['number'])
  | (ExclusiveResponseBodyBase<SV> & SV['boolean'])
  | (ExclusiveResponseBodyBase<SV> & SV['date'])
  | (ExclusiveResponseBodyBase<SV> & SV['array'])
  | (ExclusiveResponseBodyBase<SV> & SV['file']);

/**
 * Type representing the responses object in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type ResponsesObject<SV extends AnySchemaValidator> = {
  [K: number]: ResponseBody<SV>;
};

export type JsonBody<SV extends AnySchemaValidator> = {
  contentType?: 'application/json' | string;
  json: BodyObject<SV> | SV['_ValidSchemaObject'] | SV['_SchemaCatchall'];
};

/**
 * Type representing the body in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type TextBody<SV extends AnySchemaValidator> = {
  contentType?:
    | 'application/xml'
    | 'text/plain'
    | 'text/html'
    | 'text/css'
    | 'text/javascript'
    | 'text/csv'
    | 'text/markdown'
    | 'text/xml'
    | 'text/rtf'
    | 'text/x-yaml'
    | 'text/yaml'
    | string;
  text: SV['string'];
};

export type FileBody<SV extends AnySchemaValidator> = {
  contentType?:
    | 'application/octet-stream'
    | 'application/pdf'
    | 'application/vnd.ms-excel'
    | 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
    | 'application/msword'
    | 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
    | 'application/zip'
    | 'image/jpeg'
    | 'image/png'
    | 'image/gif'
    | 'audio/mpeg'
    | 'audio/wav'
    | 'video/mp4'
    | string;
  file: SV['file'];
};

/**
 * Type representing the body in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type MultipartForm<SV extends AnySchemaValidator> = {
  contentType?:
    | 'multipart/form-data'
    | 'multipart/mixed'
    | 'multipart/alternative'
    | 'multipart/related'
    | 'multipart/signed'
    | 'multipart/encrypted'
    | string;
  multipartForm: BodyObject<SV>;
};

/**
 * Type representing the body in a request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 */
export type UrlEncodedForm<SV extends AnySchemaValidator> = {
  contentType?:
    | 'application/x-www-form-urlencoded'
    | 'application/x-url-encoded'
    | 'application/x-www-url-encoded'
    | 'application/x-urlencode'
    | string;
  urlEncodedForm: BodyObject<SV>;
};

export type ServerSentEventBody<SV extends AnySchemaValidator> = {
  contentType?: 'text/event-stream' | string;
  event: {
    id: SV['string'];
    data: SV['string'] | BodyObject<SV>;
  };
};

export type UnknownBody<SV extends AnySchemaValidator> = {
  contentType?: string;
  schema: BodyObject<SV> | SV['_ValidSchemaObject'] | SV['_SchemaCatchall'];
};

export type UnknownResponseBody<SV extends AnySchemaValidator> = {
  contentType?: string;
  schema: BodyObject<SV> | SV['_ValidSchemaObject'] | SV['_SchemaCatchall'];
};

type ExclusiveRequestBodyBase<SV extends AnySchemaValidator> = {
  [K in keyof UnionToIntersection<TypedBody<SV>>]?: undefined;
};

export type TypedRequestBody<SV extends AnySchemaValidator> =
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveRequestBodyBase<SV>)]?: K extends keyof TextBody<SV>
        ? TextBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveRequestBodyBase<SV>)]?: K extends keyof JsonBody<SV>
        ? JsonBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveRequestBodyBase<SV>)]?: K extends keyof FileBody<SV>
        ? FileBody<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveRequestBodyBase<SV>)]?: K extends keyof MultipartForm<SV>
        ? MultipartForm<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveRequestBodyBase<SV>)]?: K extends keyof UrlEncodedForm<SV>
        ? UrlEncodedForm<SV>[K]
        : undefined;
    }
  | {
      [K in keyof (ExclusiveSchemaCatchall<SV> &
        ExclusiveRequestBodyBase<SV>)]?: K extends keyof UnknownBody<SV>
        ? UnknownBody<SV>[K]
        : undefined;
    };

export type TypedBody<SV extends AnySchemaValidator> =
  | JsonBody<SV>
  | TextBody<SV>
  | FileBody<SV>
  | MultipartForm<SV>
  | UrlEncodedForm<SV>
  | UnknownBody<SV>;

export type Body<SV extends AnySchemaValidator> =
  | TypedRequestBody<SV>
  | (ExclusiveRequestBodyBase<SV> & SV['_ValidSchemaObject'])
  | (ExclusiveRequestBodyBase<SV> & UnboxedObjectSchema<SV>)
  | (ExclusiveRequestBodyBase<SV> & SV['string'])
  | (ExclusiveRequestBodyBase<SV> & SV['number'])
  | (ExclusiveRequestBodyBase<SV> & SV['boolean'])
  | (ExclusiveRequestBodyBase<SV> & SV['date'])
  | (ExclusiveRequestBodyBase<SV> & SV['array'])
  | (ExclusiveRequestBodyBase<SV> & SV['file'])
  | (ExclusiveRequestBodyBase<SV> & SV['any'])
  | (ExclusiveRequestBodyBase<SV> & SV['unknown'])
  | (ExclusiveRequestBodyBase<SV> & SV['binary'])
  | (ExclusiveRequestBodyBase<SV> &
      (SV['type'] extends TypeSafeFunction ? ReturnType<SV['type']> : never));

export type SessionObject<SV extends AnySchemaValidator> = StringOnlyObject<SV>;

export type BasicAuthMethods = {
  readonly basic: {
    readonly login: (username: string, password: string) => boolean;
  };

  readonly jwt?: never;
  readonly hmac?: never;
};

export type JwtAuthMethods = {
  jwt:
    | {
        readonly jwksPublicKey: JWK;
      }
    | {
        readonly jwksPublicKeyUrl: string;
      }
    | {
        readonly signatureKey: string;
      };
  readonly basic?: never;
  readonly hmac?: never;
};

export type HmacMethods = {
  readonly hmac: {
    readonly secretKeys: Record<string, string>;
  };

  readonly sessionSchema?: never;
  readonly basic?: never;
  readonly jwt?: never;
};

type TokenOptions = {
  readonly tokenPrefix?: string;
  readonly headerName?: string;
};

export type DecodeResource = (
  token: string
) => JWTPayload | Promise<JWTPayload>;

export type PermissionSet =
  | {
      readonly allowedPermissions: Set<string>;
    }
  | { readonly forbiddenPermissions: Set<string> };

export type RoleSet =
  | {
      readonly allowedRoles: Set<string>;
    }
  | {
      readonly forbiddenRoles: Set<string>;
    };

/** Constraints that forbid RBAC fields on authenticated routes. */
type NoRbacConstraint = {
  readonly allowedPermissions?: never;
  readonly forbiddenPermissions?: never;
  readonly allowedRoles?: never;
  readonly forbiddenRoles?: never;
};

/**
 * @deprecated Use AccessAuth discriminated union instead.
 * Kept for backwards compatibility with AuthMethods.
 */
export type AuthMethodsBase = TokenOptions &
  (
    | HmacMethods
    | ({
        readonly decodeResource?: DecodeResource;
      } & (
        | (PermissionSet & (BasicAuthMethods | JwtAuthMethods))
        | (RoleSet & (BasicAuthMethods | JwtAuthMethods))
        | (BasicAuthMethods | JwtAuthMethods)
      ))
  );

/**
 * Route access level — determines authentication and authorization requirements.
 *
 * - `'public'` — No authentication required. `auth` must not be provided.
 * - `'authenticated'` — JWT or Basic auth required, any valid user. RBAC not allowed.
 * - `'protected'` — JWT or Basic auth required. Must declare roles, permissions, or scope.
 * - `'internal'` — HMAC auth required (inter-service communication).
 */
export type AccessLevel = 'public' | 'authenticated' | 'protected' | 'internal';

/**
 * Discriminated union that narrows the `auth` type based on the `access` level.
 * - `public`: auth not allowed
 * - `authenticated`: JWT/Basic auth required, RBAC fields disallowed
 * - `protected`: auth required, must include at least one RBAC declaration
 * - `internal`: auth required, must be HMAC
 */
type AccessAuth<Auth> =
  | { readonly access: 'public'; readonly auth?: never }
  | {
      readonly access: 'authenticated';
      readonly auth?: Auth & NoRbacConstraint;
    }
  | {
      readonly access: 'protected';
      readonly auth: Auth &
        (
          | { readonly allowedPermissions: Set<string> }
          | { readonly forbiddenPermissions: Set<string> }
          | { readonly allowedRoles: Set<string> }
          | { readonly forbiddenRoles: Set<string> }
          | { readonly requiredScope: string }
        );
    }
  | { readonly access: 'internal'; readonly auth: Auth & HmacMethods };

/**
 * Type representing the authentication methods.
 */
export type SchemaAuthMethods<
  SV extends AnySchemaValidator,
  ParamsSchema extends ParamsObject<SV>,
  ReqBody extends Body<SV>,
  QuerySchema extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  VersionedApi extends VersionSchema<SV, Method>,
  BaseRequest
> = AuthMethodsBase & {
  readonly sessionSchema?: SessionObject<SV>;
  readonly requiredScope?: string;
  readonly scopeHeirarchy?: string[];
  readonly surfaceScopes?: ExpressLikeSchemaAuthMapper<
    SV,
    ParamsSchema,
    ReqBody,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    SessionObject<SV>,
    BaseRequest
  >;
  readonly requiredFeatures?: string[];
  readonly requireActiveSubscription?: boolean;
} & (
    | {
        readonly surfacePermissions?: ExpressLikeSchemaAuthMapper<
          SV,
          ParamsSchema,
          ReqBody,
          QuerySchema,
          ReqHeaders,
          VersionedApi,
          SessionObject<SV>,
          BaseRequest
        >;
      }
    | {
        readonly surfaceRoles?: ExpressLikeSchemaAuthMapper<
          SV,
          ParamsSchema,
          ReqBody,
          QuerySchema,
          ReqHeaders,
          VersionedApi,
          SessionObject<SV>,
          BaseRequest
        >;
      }
  );

export type AuthMethods<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends Record<string, unknown>,
  ReqHeaders extends Record<string, string>,
  VersionedReqs extends VersionedRequests,
  BaseRequest
> = AuthMethodsBase & {
  readonly sessionSchema?: SessionObject<SV>;
  readonly requiredScope?: string;
  readonly scopeHeirarchy?: string[];
  readonly surfaceScopes?: ExpressLikeAuthMapper<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    VersionedReqs,
    SessionObject<SV>,
    BaseRequest
  >;
  readonly requiredFeatures?: string[];
  readonly requireActiveSubscription?: boolean;
} & (
    | ({
        readonly surfacePermissions?: ExpressLikeAuthMapper<
          SV,
          P,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          VersionedReqs,
          SessionObject<SV>,
          BaseRequest
        >;
      } & PermissionSet)
    | ({
        readonly surfaceRoles?: ExpressLikeAuthMapper<
          SV,
          P,
          ReqBody,
          ReqQuery,
          ReqHeaders,
          VersionedReqs,
          SessionObject<SV>,
          BaseRequest
        >;
      } & RoleSet)
  );

/**
 * Type representing a mapped schema.
 *s ParamsDictionary,
//   ReqBody extends Record<string, unknown>,
// 
 * @template SV - A type that extends AnySchemaValidator.
 * @template T - A type that extends IdiomaticSchema or a valid schema object.
 */
export type MapSchema<
  SV extends AnySchemaValidator,
  T extends IdiomaticSchema<SV> | SV['_ValidSchemaObject']
> = Schema<T, SV> extends infer U ? (T extends U ? unknown : U) : never;

/**
 * Type representing the parameters in a request.
 */
type ExtractParams<Path extends `/${string}`> =
  Path extends `${string}/:${infer Param}/${infer Rest}`
    ? Param | ExtractParams<`/${Rest}`>
    : Path extends `${string}/:${infer Param}`
      ? Param
      : never;

/**
 * Type representing the parameters in a request.
 */
export type ExtractedParamsObject<Path extends `/${string}`> = Record<
  ExtractParams<Path>,
  unknown
>;

/**
 * Represents the path parameter methods.
 */
export type PathParamMethod = 'get' | 'delete' | 'options' | 'head' | 'trace';

/**
 * Represents the body parameter methods.
 */
export type HttpMethod = 'post' | 'patch' | 'put';

/**
 * Represents all supported typed methods.
 */
export type Method = PathParamMethod | HttpMethod | 'middleware';

/**
 * Interface representing a compiled schema for a response.
 */
export type ResponseCompiledSchema = {
  headers?: unknown;
  responses: Record<number, unknown>;
};

type BasePathParamHttpContractDetailsIO<
  SV extends AnySchemaValidator,
  BodySchema extends Body<SV> | undefined = Body<SV>,
  ResponseSchemas extends ResponsesObject<SV> = ResponsesObject<SV>,
  QuerySchema extends QueryObject<SV> | undefined = QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV> | undefined = HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV> | undefined = HeadersObject<SV>
> = {
  /** Optional body for the contract */
  readonly body?: BodySchema;
  /** Response schemas for the contract */
  readonly responses: ResponseSchemas;
  /** Optional request headers for the contract */
  readonly requestHeaders?: ReqHeaders;
  /** Optional response headers for the contract */
  readonly responseHeaders?: ResHeaders;
  /** Optional query schemas for the contract */
  readonly query?: QuerySchema;
};

type VersionedBasePathParamHttpContractDetailsIO<
  SV extends AnySchemaValidator,
  VersionedApi extends VersionSchema<SV, PathParamMethod>
> = {
  readonly versions: VersionedApi;
};

type BasePathParamHttpContractDetails<
  SV extends AnySchemaValidator,
  Name extends string = string,
  Path extends `/${string}` = `/${string}`,
  ParamsSchema extends ParamsObject<SV> = ParamsObject<SV>
> = {
  /** Name of the contract */
  readonly name: StringWithoutSlash<Name>;
  /** Summary of the contract */
  readonly summary: string;
  /** Options for the contract */
  readonly options?: {
    /** Optional request validation for the contract */
    readonly requestValidation?: 'error' | 'warning' | 'none';
    /** Optional response validation for the contract */
    readonly responseValidation?: 'error' | 'warning' | 'none';
    /** Optional MCP details for the contract */
    readonly mcp?: boolean;
    /** Optional OpenAPI details for the contract */
    readonly openapi?: boolean;
  };
} & (string | number | symbol extends ExtractedParamsObject<Path>
  ? {
      /** Optional parameters for the contract */
      readonly params?: ParamsSchema;
    }
  : {
      /** Required parameters for the contract */
      readonly params: {
        [K in keyof ExtractedParamsObject<Path>]: ParamsSchema[K];
      };
    });

/**
 * Interface representing HTTP contract details for path parameters.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template ParamsSchema - A type for parameter schemas, defaulting to ParamsObject.
 * @template ResponseSchemas - A type for response schemas, defaulting to ResponsesObject.
 * @template QuerySchema - A type for query schemas, defaulting to QueryObject.
 */
export type PathParamHttpContractDetails<
  SV extends AnySchemaValidator,
  Name extends string = string,
  Path extends `/${string}` = `/${string}`,
  ParamsSchema extends ParamsObject<SV> = ParamsObject<SV>,
  ResponseSchemas extends ResponsesObject<SV> = ResponsesObject<SV>,
  BodySchema extends Body<SV> = Body<SV>,
  QuerySchema extends QueryObject<SV> = QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV> = HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV> = HeadersObject<SV>,
  VersionedApi extends VersionSchema<SV, Method> = VersionSchema<
    SV,
    PathParamMethod
  >,
  BaseRequest = unknown,
  Auth extends SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  > = SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >
> = BasePathParamHttpContractDetails<SV, Name, Path, ParamsSchema> &
  (
    | (BasePathParamHttpContractDetailsIO<
        SV,
        never,
        ResponseSchemas,
        QuerySchema,
        ReqHeaders,
        ResHeaders
      > & {
        readonly versions?: never;
      })
    | (VersionedBasePathParamHttpContractDetailsIO<SV, VersionedApi> & {
        readonly query?: never;
        readonly requestHeaders?: never;
        readonly responseHeaders?: never;
        readonly responses?: never;
      })
  ) &
  AccessAuth<Auth>;

type VersionedHttpContractDetailsIO<
  SV extends AnySchemaValidator,
  VersionedApi extends VersionSchema<SV, HttpMethod>
> = {
  readonly versions: VersionedApi;
};

/**
 * Interface representing HTTP contract details.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template ParamsSchema - A type for parameter schemas, defaulting to ParamsObject.
 * @template ResponseSchemas - A type for response schemas, defaulting to ResponsesObject.
 * @template BodySchema - A type for the body schema, defaulting to Body.
 * @template QuerySchema - A type for query schemas, defaulting to QueryObject.
 */
export type HttpContractDetails<
  SV extends AnySchemaValidator,
  Name extends string = string,
  Path extends `/${string}` = `/${string}`,
  ParamsSchema extends ParamsObject<SV> = ParamsObject<SV>,
  ResponseSchemas extends ResponsesObject<SV> = ResponsesObject<SV>,
  BodySchema extends Body<SV> = Body<SV>,
  QuerySchema extends QueryObject<SV> = QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV> = HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV> = HeadersObject<SV>,
  VersionedApi extends VersionSchema<SV, Method> = VersionSchema<
    SV,
    HttpMethod
  >,
  BaseRequest = unknown,
  Auth extends SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  > = SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >
> = BasePathParamHttpContractDetails<SV, Name, Path, ParamsSchema> &
  (
    | (BasePathParamHttpContractDetailsIO<
        SV,
        BodySchema,
        ResponseSchemas,
        QuerySchema,
        ReqHeaders,
        ResHeaders
      > & {
        readonly versions?: never;
      })
    | (VersionedHttpContractDetailsIO<SV, VersionedApi> & {
        readonly query?: never;
        readonly requestHeaders?: never;
        readonly responseHeaders?: never;
        readonly body?: never;
        readonly responses?: never;
      })
  ) &
  AccessAuth<Auth>;

/**
 * Interface representing HTTP contract details for middleware.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template ParamsSchema - A type for parameter schemas, defaulting to ParamsObject.
 * @template ResponseSchemas - A type for response schemas, defaulting to ResponsesObject.
 * @template QuerySchema - A type for query schemas, defaulting to QueryObject.
 * @template ReqHeaders - A type for request headers, defaulting to HeadersObject.
 * @template ResHeaders - A type for response headers, defaulting to HeadersObject.
 */
export type MiddlewareContractDetails<
  SV extends AnySchemaValidator,
  Name extends string = string,
  Path extends `/${string}` = `/${string}`,
  ParamsSchema extends ParamsObject<SV> = ParamsObject<SV>,
  ResponseSchemas extends ResponsesObject<SV> = ResponsesObject<SV>,
  BodySchema extends Body<SV> = Body<SV>,
  QuerySchema extends QueryObject<SV> = QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV> = HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV> = HeadersObject<SV>,
  VersionedApi extends VersionSchema<SV, Method> = VersionSchema<
    SV,
    'middleware'
  >,
  BaseRequest = unknown,
  Auth extends SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  > = SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >
> = Omit<
  Partial<
    HttpContractDetails<
      SV,
      Name,
      Path,
      ParamsSchema,
      ResponseSchemas,
      BodySchema,
      QuerySchema,
      ReqHeaders,
      ResHeaders,
      VersionedApi,
      BaseRequest,
      Auth
    >
  >,
  'responses'
>;

export type VersionSchema<
  SV extends AnySchemaValidator,
  ContractMethod extends Method
> = Record<
  string,
  BasePathParamHttpContractDetailsIO<
    SV,
    ContractMethod extends PathParamMethod ? never : Body<SV>,
    ResponsesObject<SV>,
    QueryObject<SV>,
    HeadersObject<SV>,
    HeadersObject<SV>
  >
>;

/**
 * Utility for different Contract Detail types
 */
export type ContractDetails<
  SV extends AnySchemaValidator,
  Name extends string,
  ContractMethod extends Method,
  Path extends `/${string}`,
  ParamsSchema extends ParamsObject<SV>,
  ResponseSchemas extends ResponsesObject<SV>,
  BodySchema extends Body<SV>,
  QuerySchema extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  VersionedApi extends VersionSchema<SV, ContractMethod>,
  BaseRequest,
  Auth extends SchemaAuthMethods<
    SV,
    ParamsSchema,
    BodySchema,
    QuerySchema,
    ReqHeaders,
    VersionedApi,
    BaseRequest
  >
> = ContractMethod extends PathParamMethod
  ? PathParamHttpContractDetails<
      SV,
      Name,
      Path,
      ParamsSchema,
      ResponseSchemas,
      BodySchema,
      QuerySchema,
      ReqHeaders,
      ResHeaders,
      VersionedApi,
      BaseRequest,
      Auth
    >
  : ContractMethod extends HttpMethod
    ? HttpContractDetails<
        SV,
        Name,
        Path,
        ParamsSchema,
        ResponseSchemas,
        BodySchema,
        QuerySchema,
        ReqHeaders,
        ResHeaders,
        VersionedApi,
        BaseRequest,
        Auth
      >
    : ContractMethod extends 'middleware'
      ? MiddlewareContractDetails<
          SV,
          Name,
          Path,
          ParamsSchema,
          ResponseSchemas,
          BodySchema,
          QuerySchema,
          ReqHeaders,
          ResHeaders,
          VersionedApi,
          BaseRequest,
          Auth
        >
      : never;

/**
 * Resolves the session object type to use for authentication.
 *
 * If `AuthSession` is provided and extends `SessionObject<SV>`, it is used as the session type.
 * Otherwise, the `FallbackSession` type is used.
 *
 * @template SV - The schema validator type.
 * @template AuthSession - The session object type provided by the authentication method, or undefined.
 * @template FallbackSession - The fallback session object type to use if `AuthSession` is not provided.
 */
export type ResolvedSessionObject<
  SV extends AnySchemaValidator,
  AuthSession extends SessionObject<SV> | undefined,
  FallbackSession extends SessionObject<SV>
> = AuthSession extends
  | {
      readonly sessionSchema?: infer ResolvedSessionSchema | undefined;
    }
  | undefined
  ? ResolvedSessionSchema extends SessionObject<SV>
    ? ResolvedSessionSchema
    : FallbackSession
  : FallbackSession;
