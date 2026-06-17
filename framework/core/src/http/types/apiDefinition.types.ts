import {
  MakePropertyOptionalIfChildrenOptional,
  Prettify,
  SanitizePathSlashes
} from '@forklaunch/common';
import { AnySchemaValidator } from '@forklaunch/validator';
import { Span } from '@opentelemetry/api';
import { JWTPayload } from 'jose';
import { ParsedQs } from 'qs';
import { Readable } from 'stream';
import { OpenTelemetryCollector } from '../telemetry/openTelemetryCollector';
import {
  AuthMethodsBase,
  BasicAuthMethods,
  Body,
  FileBody,
  HeadersObject,
  HmacMethods,
  HttpContractDetails,
  JsonBody,
  MapSchema,
  Method,
  MultipartForm,
  ParamsDictionary,
  ParamsObject,
  PathParamHttpContractDetails,
  QueryObject,
  ResponseBody,
  ResponseCompiledSchema,
  ResponsesObject,
  ServerSentEventBody,
  SessionObject,
  TextBody,
  UnknownBody,
  UnknownResponseBody,
  UrlEncodedForm,
  VersionSchema
} from './contractDetails.types';
import { ExpressLikeRouterOptions } from './expressLikeOptions';
import { MetricsDefinition } from './openTelemetryCollector.types';

/**
 * Interface representing the context of a request.
 */
export interface RequestContext {
  /** Correlation ID for tracking requests */
  correlationId: string;
  /** Optional idempotency key for ensuring idempotent requests */
  idempotencyKey?: string;
  /** Active OpenTelemetry Span */
  span?: Span;
  /** Wall-clock timestamp (ms) when the request context was created, used for duration metrics */
  requestStartTime?: number;
}

export interface ForklaunchBaseRequest<
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends ParsedQs,
  ReqHeaders extends Record<string, string>
> {
  /** Request parameters */
  params: P;
  /** Request headers */
  headers: ReqHeaders;
  /** Request body */
  body: ReqBody;
  /** Request query */
  query: ReqQuery;

  /** Method */
  method:
    | 'GET'
    | 'POST'
    | 'PUT'
    | 'PATCH'
    | 'DELETE'
    | 'OPTIONS'
    | 'HEAD'
    | 'CONNECT'
    | 'TRACE';

  /** Request path */
  path: string;

  /** Original path */
  originalPath: string;

  /** OpenTelemetry Collector */
  openTelemetryCollector?: OpenTelemetryCollector<MetricsDefinition>;
}

/**
 * Interface representing a Forklaunch request.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template P - A type for request parameters, defaulting to ParamsDictionary.
 * @template ReqBody - A type for the request body, defaulting to unknown.
 * @template ReqQuery - A type for the request query, defaulting to ParsedQs.
 * @template Headers - A type for the request headers, defaulting to IncomingHttpHeaders.
 */
export interface ForklaunchRequest<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends Record<string, unknown>,
  ReqHeaders extends Record<string, unknown>,
  Version extends string,
  SessionSchema extends Record<string, unknown>
> {
  /** Context of the request */
  context: Prettify<RequestContext>;

  /** API Version of the request */
  version: Version;

  /** Request parameters */
  params: P;
  /** Request headers */
  headers: ReqHeaders;
  /** Request body */
  body: ReqBody;
  /** Request query */
  query: ReqQuery;

  /** Contract details for the request */
  contractDetails: PathParamHttpContractDetails<SV> | HttpContractDetails<SV>;
  /** Schema validator */
  schemaValidator: SV;

  /** Method */
  method:
    | 'GET'
    | 'POST'
    | 'PUT'
    | 'PATCH'
    | 'DELETE'
    | 'OPTIONS'
    | 'HEAD'
    | 'CONNECT'
    | 'TRACE';

  /** Request path */
  path: string;

  /** Request schema, compiled */
  requestSchema: unknown | Record<string, unknown>;

  /** Original path */
  originalPath: string;

  /** OpenTelemetry Collector */
  openTelemetryCollector?: OpenTelemetryCollector<MetricsDefinition>;

  /** Session */
  session: JWTPayload & SessionSchema;

  /** Parsed versions */
  _parsedVersions: string[] | number;

  /** Raw body before schema validation (for HMAC verification) */
  _rawBody?: unknown;

  /** Global options */
  _globalOptions: () => ExpressLikeRouterOptions<SV, SessionSchema> | undefined;
}

/**
 * Represents the types of data that can be sent in a response.
 */
export type ForklaunchSendableData =
  | Record<string, unknown>
  | string
  | Buffer
  | ArrayBuffer
  | NodeJS.ReadableStream
  | null
  | undefined;

/**
 * Interface representing a Forklaunch response status.
 * @template ResBody - A type for the response body.
 */
export interface ForklaunchStatusResponse<ResBody> {
  /**
   * Sends the response.
   * @param {ResBodyMap} [body] - The response body.
   * @param {boolean} [close_connection] - Whether to close the connection.
   * @returns {T} - The sent response.
   */
  send: {
    (
      body?: ResBody extends AsyncGenerator<unknown>
        ? never
        : ResBody extends Blob
          ? Blob | File | Buffer | ArrayBuffer | NodeJS.ReadableStream
          : ResBody | null,
      close_connection?: boolean
    ): boolean;
    <U>(
      body?: ResBody extends AsyncGenerator<unknown>
        ? never
        : ResBody extends Blob
          ? Blob | File | Buffer | ArrayBuffer | NodeJS.ReadableStream
          : ResBody | null,
      close_connection?: boolean
    ): U;
  };

  /**
   * Sends a JSON response.
   * @param {ResBodyMap} [body] - The response body.
   * @returns {boolean|T} - The JSON response.
   */
  json: {
    (
      body: ResBody extends string | AsyncGenerator<unknown>
        ? never
        : ResBody | null
    ): boolean;
    <U>(
      body: ResBody extends string | AsyncGenerator<unknown>
        ? never
        : ResBody | null
    ): U;
  };

  /**
   * Sends a JSONP response.
   * @param {ResBodyMap} [body] - The response body.
   * @returns {boolean|T} - The JSONP response.
   */
  jsonp: {
    (
      body: ResBody extends string | AsyncGenerator<unknown>
        ? never
        : ResBody | null
    ): boolean;
    <U>(
      body: ResBody extends string | AsyncGenerator<unknown>
        ? never
        : ResBody | null
    ): U;
  };

  /**
   * Sends a Server-Sent Event (SSE) response.
   * @param {ResBodyMap} [body] - The response body.
   * @param {number} interval - The interval between events.
   */
  sseEmitter: (
    generator: () => AsyncGenerator<
      ResBody extends AsyncGenerator<infer T> ? T : never,
      void,
      unknown
    >
  ) => void;
}

type ToNumber<T extends string | number | symbol> = T extends number
  ? T
  : T extends `${infer N extends number}`
    ? N
    : never;

/**
 * Interface representing a Forklaunch response.
 *
 * @template ResBodyMap - A type for the response body, defaulting to common status code responses.
 * @template StatusCode - A type for the status code, defaulting to number.
 */
export interface ForklaunchResponse<
  BaseResponse,
  ResBodyMap extends Record<number, unknown>,
  ResHeaders extends Record<string, unknown>,
  LocalsObj extends Record<string, unknown>,
  Version extends string
> {
  /** Data of the response body */
  bodyData: unknown;
  /** Status code of the response */
  statusCode: number;
  /** Whether the response is finished */
  headersSent: boolean;

  /**
   * Gets the headers of the response.
   * @returns {Omit<ResHeaders, keyof ForklaunchResHeaders> & ForklaunchResHeaders} - The headers of the response.
   */
  getHeaders: () => Omit<ResHeaders, keyof ForklaunchResHeaders> &
    ForklaunchResHeaders;

  /**
   * Gets a header for the response.
   * @param {string} key - The header key.
   * @returns {string | string[] | undefined} The header value.
   */
  getHeader: (key: string) => string | string[] | undefined;

  /**
   * Sets a header for the response.
   * @param {string} key - The header key.
   * @param {string} value - The header value.
   */
  setHeader: {
    <K extends keyof (ResHeaders & ForklaunchResHeaders)>(
      key: K,
      value: K extends keyof ForklaunchResHeaders
        ? ForklaunchResHeaders[K]
        : ResHeaders[K]
    ): void;
    <K extends keyof (ResHeaders & ForklaunchResHeaders)>(
      key: K,
      value: K extends keyof ForklaunchResHeaders
        ? ForklaunchResHeaders[K]
        : ResHeaders[K]
    ): BaseResponse;
  };

  /**
   * Adds an event listener to the response.
   * @param {string} event - The event to listen for.
   * @param {Function} listener - The listener function.
   */
  on(event: 'close', listener: () => void): BaseResponse & this;
  on(event: 'drain', listener: () => void): BaseResponse & this;
  on(event: 'error', listener: (err: Error) => void): BaseResponse & this;
  on(event: 'finish', listener: () => void): BaseResponse & this;
  on(event: 'pipe', listener: (src: Readable) => void): BaseResponse & this;
  on(event: 'unpipe', listener: (src: Readable) => void): BaseResponse & this;
  on(
    event: string | symbol,
    listener: (...args: unknown[]) => void
  ): BaseResponse & this;

  /**
   * Sets the status of the response.
   * @param {U} code - The status code.
   * @param {string} [message] - Optional message.
   * @returns {ForklaunchResponse<(ResBodyMap)[U], ResHeaders, U, LocalsObj>} - The response with the given status.
   */
  status: {
    <U extends ToNumber<keyof (ResBodyMap & ForklaunchResErrors)>>(
      code: U
    ): Omit<
      BaseResponse,
      keyof ForklaunchStatusResponse<
        (Omit<ForklaunchResErrors, keyof ResBodyMap> & ResBodyMap)[U]
      >
    > &
      ForklaunchStatusResponse<
        (Omit<ForklaunchResErrors, keyof ResBodyMap> & ResBodyMap)[U]
      >;
    <U extends ToNumber<keyof (ResBodyMap & ForklaunchResErrors)>>(
      code: U,
      message?: string
    ): Omit<
      BaseResponse,
      keyof ForklaunchStatusResponse<
        (Omit<ForklaunchResErrors, keyof ResBodyMap> & ResBodyMap)[U]
      >
    > &
      ForklaunchStatusResponse<
        (Omit<ForklaunchResErrors, keyof ResBodyMap> & ResBodyMap)[U]
      >;
  };

  /**
   * Ends the response.
   * @param {string} [data] - Optional data to send.
   */
  end: {
    (data?: string): void;
    (cb?: (() => void) | undefined): BaseResponse;
  };

  /**
   * Sets the content type of the response.
   * @param {string} type - The content type.
   */
  type: {
    (type: string): void;
    (type: string): BaseResponse;
  };

  /** Local variables */
  locals: LocalsObj;

  /** Cors */
  cors: boolean;

  /** Response schema, compiled */
  responseSchemas:
    | ResponseCompiledSchema
    | Record<string, ResponseCompiledSchema>;

  /** Whether the metric has been recorded */
  metricRecorded: boolean;

  /** Whether the response has been sent */
  sent: boolean;

  /** Versioned responses */
  version: Version;
}

/**
 * Type representing the next function in a middleware.
 * @param {unknown} [err] - Optional error parameter.
 */
export type ForklaunchNextFunction = (err?: unknown) => void;

export type VersionedRequests = Record<
  string,
  {
    requestHeaders?: Record<string, unknown>;
    body?: Record<string, unknown>;
    query?: Record<string, unknown>;
  }
>;

type ResolvedForklaunchRequestBase<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends Record<string, unknown>,
  ReqHeaders extends Record<string, unknown>,
  Version extends string,
  SessionSchema extends Record<string, unknown>,
  BaseRequest
> = unknown extends BaseRequest
  ? ForklaunchRequest<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      Version,
      SessionSchema
    >
  : Omit<
      BaseRequest,
      keyof ForklaunchRequest<
        SV,
        P,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        Version,
        SessionSchema
      >
    > &
      ForklaunchRequest<
        SV,
        P,
        ReqBody,
        ReqQuery,
        ReqHeaders,
        Version,
        SessionSchema
      >;

/**
 * Type representing the resolved forklaunch request from a base request type.
 * @template SV - A type that extends AnySchemaValidator.
 * @template P - A type for request parameters, defaulting to ParamsDictionary.
 * @template ReqBody - A type for the request body, defaulting to Record<string, unknown>.
 * @template ReqQuery - A type for the request query, defaulting to ParsedQs.
 * @template ReqHeaders - A type for the request headers, defaulting to Record<string, unknown>.
 * @template BaseRequest - A type for the base request.
 */
export type ResolvedForklaunchRequest<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends Record<string, unknown>,
  ReqHeaders extends Record<string, string>,
  VersionedReqs extends VersionedRequests,
  SessionSchema extends Record<string, unknown>,
  BaseRequest
> = VersionedRequests extends VersionedReqs
  ? ResolvedForklaunchRequestBase<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      never,
      SessionSchema,
      BaseRequest
    >
  : {
      [K in keyof VersionedReqs]: ResolvedForklaunchRequestBase<
        SV,
        P,
        VersionedReqs[K]['body'] extends Record<string, unknown>
          ? VersionedReqs[K]['body']
          : Record<string, unknown>,
        VersionedReqs[K]['query'] extends Record<string, unknown>
          ? VersionedReqs[K]['query']
          : ParsedQs,
        VersionedReqs[K]['requestHeaders'] extends Record<string, unknown>
          ? VersionedReqs[K]['requestHeaders']
          : Record<string, string>,
        K extends string ? K : never,
        SessionSchema,
        BaseRequest
      >;
    }[keyof VersionedReqs];

export type ResolvedForklaunchAuthRequest<
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends ParsedQs,
  ReqHeaders extends Record<string, string>,
  BaseRequest
> = unknown extends BaseRequest
  ? ForklaunchBaseRequest<P, ReqBody, ReqQuery, ReqHeaders>
  : {
      [key in keyof BaseRequest]: key extends keyof ForklaunchBaseRequest<
        P,
        ReqBody,
        ReqQuery,
        ReqHeaders
      >
        ? ForklaunchBaseRequest<P, ReqBody, ReqQuery, ReqHeaders>[key]
        : key extends keyof BaseRequest
          ? BaseRequest[key]
          : never;
    };

export type VersionedResponses = Record<
  string,
  {
    responseHeaders?: Record<string, unknown>;
    responses: Record<number, unknown>;
  }
>;

type ResolvedForklaunchResponseBase<
  ResBodyMap extends Record<number, unknown>,
  ResHeaders extends Record<string, unknown>,
  LocalsObj extends Record<string, unknown>,
  Version extends string,
  BaseResponse
> = unknown extends BaseResponse
  ? ForklaunchResponse<BaseResponse, ResBodyMap, ResHeaders, LocalsObj, Version>
  : (string extends Version ? unknown : { version?: Version }) & {
      [K in
        | keyof BaseResponse
        | keyof ForklaunchResponse<
            BaseResponse,
            ResBodyMap,
            ResHeaders,
            LocalsObj,
            Version
          >]: K extends keyof ForklaunchResponse<
        BaseResponse,
        ResBodyMap,
        ResHeaders,
        LocalsObj,
        Version
      >
        ? ForklaunchResponse<
            BaseResponse,
            ResBodyMap,
            ResHeaders,
            LocalsObj,
            Version
          >[K]
        : K extends keyof BaseResponse
          ? BaseResponse[K]
          : never;
    };

export type ResolvedForklaunchResponse<
  ResBodyMap extends Record<number, unknown>,
  ResHeaders extends Record<string, string>,
  LocalsObj extends Record<string, unknown>,
  VersionedResps extends VersionedResponses,
  BaseResponse
> = VersionedResponses extends VersionedResps
  ? ResolvedForklaunchResponseBase<
      ResBodyMap,
      ResHeaders,
      LocalsObj,
      never,
      BaseResponse
    >
  : {
      [K in keyof VersionedResps]: ResolvedForklaunchResponseBase<
        VersionedResps[K]['responses'],
        VersionedResps[K]['responseHeaders'] extends Record<string, unknown>
          ? VersionedResps[K]['responseHeaders']
          : Record<string, string>,
        LocalsObj,
        K extends string ? K : never,
        BaseResponse
      >;
    }[keyof VersionedResps];

/**
 * Represents a middleware handler with schema validation.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template P - A type for request parameters, defaulting to ParamsDictionary.
 * @template ResBodyMap - A type for the response body, defaulting to unknown.
 * @template ReqBody - A type for the request body, defaulting to unknown.
 * @template ReqQuery - A type for the request query, defaulting to ParsedQs.
 * @template LocalsObj - A type for local variables, defaulting to an empty object.
 * @template StatusCode - A type for the status code, defaulting to number.
 */
export interface ExpressLikeHandler<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ResBodyMap extends Record<number, unknown>,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends ParsedQs,
  ReqHeaders extends Record<string, string>,
  ResHeaders extends Record<string, string>,
  LocalsObj extends Record<string, unknown>,
  VersionedReqs extends VersionedRequests,
  VersionedResps extends VersionedResponses,
  SessionSchema extends Record<string, unknown>,
  BaseRequest,
  BaseResponse,
  NextFunction
> {
  (
    req: ResolvedForklaunchRequest<
      SV,
      P,
      ReqBody,
      ReqQuery,
      ReqHeaders,
      VersionedReqs,
      SessionSchema,
      BaseRequest
    >,
    res: ResolvedForklaunchResponse<
      ResBodyMap,
      ResHeaders,
      LocalsObj,
      VersionedResps,
      BaseResponse
    >,
    next: NextFunction
  ): unknown;
}

export type MapParamsSchema<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>
> =
  MapSchema<SV, P> extends infer Params
    ? unknown extends Params
      ? ParamsDictionary
      : Params
    : ParamsDictionary;

export type ExtractContentType<
  SV extends AnySchemaValidator,
  T extends ResponseBody<SV> | unknown
> = T extends { contentType: string }
  ? T['contentType']
  : T extends JsonBody<SV>
    ? 'application/json'
    : T extends TextBody<SV>
      ? 'text/plain'
      : T extends FileBody<SV>
        ? 'application/octet-stream'
        : T extends ServerSentEventBody<SV>
          ? 'text/event-stream'
          : T extends UnknownResponseBody<SV>
            ? 'application/json'
            : T extends SV['file']
              ? 'application/octet-stream'
              : 'text/plain';

export type ExtractResponseBody<
  SV extends AnySchemaValidator,
  T extends ResponseBody<SV> | unknown
> =
  T extends JsonBody<SV>
    ? MapSchema<SV, T['json']>
    : T extends TextBody<SV>
      ? MapSchema<SV, T['text']>
      : T extends FileBody<SV>
        ? File | Blob
        : T extends ServerSentEventBody<SV>
          ? AsyncGenerator<MapSchema<SV, T['event']>>
          : T extends UnknownResponseBody<SV>
            ? MapSchema<SV, T['schema']>
            : MapSchema<SV, T>;

export type MapResBodyMapSchema<
  SV extends AnySchemaValidator,
  ResBodyMap extends ResponsesObject<SV>
> = unknown extends ResBodyMap
  ? ForklaunchResErrors
  : {
      [K in keyof ResBodyMap]: ExtractResponseBody<SV, ResBodyMap[K]>;
    };

export type ExtractBody<SV extends AnySchemaValidator, T extends Body<SV>> =
  T extends JsonBody<SV>
    ? T['json']
    : T extends TextBody<SV>
      ? T['text']
      : T extends FileBody<SV>
        ? T['file']
        : T extends MultipartForm<SV>
          ? T['multipartForm']
          : T extends UrlEncodedForm<SV>
            ? T['urlEncodedForm']
            : T extends UnknownBody<SV>
              ? T['schema']
              : T;

export type MapReqBodySchema<
  SV extends AnySchemaValidator,
  ReqBody extends Body<SV>
> =
  MapSchema<SV, ExtractBody<SV, ReqBody>> extends infer Body
    ? unknown extends Body
      ? Record<string, unknown>
      : Body
    : Record<string, unknown>;

export type MapReqQuerySchema<
  SV extends AnySchemaValidator,
  ReqQuery extends QueryObject<SV>
> =
  MapSchema<SV, ReqQuery> extends infer Query
    ? unknown extends Query
      ? ParsedQs
      : Query
    : ParsedQs;

export type MapReqHeadersSchema<
  SV extends AnySchemaValidator,
  ReqHeaders extends HeadersObject<SV>
> =
  MapSchema<SV, ReqHeaders> extends infer RequestHeaders
    ? unknown extends RequestHeaders
      ? Record<string, string>
      : RequestHeaders
    : Record<string, string>;

export type MapResHeadersSchema<
  SV extends AnySchemaValidator,
  ResHeaders extends HeadersObject<SV>
> =
  MapSchema<SV, ResHeaders> extends infer ResponseHeaders
    ? unknown extends ResponseHeaders
      ? ForklaunchResHeaders
      : ResponseHeaders
    : ForklaunchResHeaders;

export type MapVersionedReqsSchema<
  SV extends AnySchemaValidator,
  VersionedReqs extends VersionSchema<SV, Method>
> = {
  [K in keyof VersionedReqs]: (VersionedReqs[K]['requestHeaders'] extends HeadersObject<SV>
    ? {
        requestHeaders: MapReqHeadersSchema<
          SV,
          VersionedReqs[K]['requestHeaders']
        >;
      }
    : unknown) &
    (VersionedReqs[K]['body'] extends Body<SV>
      ? {
          body: MapReqBodySchema<SV, VersionedReqs[K]['body']>;
        }
      : unknown) &
    (VersionedReqs[K]['query'] extends QueryObject<SV>
      ? {
          query: MapReqQuerySchema<SV, VersionedReqs[K]['query']>;
        }
      : unknown);
} extends infer MappedVersionedReqs
  ? MappedVersionedReqs extends VersionedRequests
    ? MappedVersionedReqs
    : VersionedRequests
  : VersionedRequests;

export type MapVersionedRespsSchema<
  SV extends AnySchemaValidator,
  VersionedResps extends VersionSchema<SV, Method>
> = {
  [K in keyof VersionedResps]: (VersionedResps[K]['responseHeaders'] extends HeadersObject<SV>
    ? {
        responseHeaders: MapResHeadersSchema<
          SV,
          VersionedResps[K]['responseHeaders']
        >;
      }
    : unknown) &
    (VersionedResps[K]['responses'] extends ResponsesObject<SV>
      ? {
          responses: MapResBodyMapSchema<SV, VersionedResps[K]['responses']>;
        }
      : unknown);
} extends infer MappedVersionedResps
  ? MappedVersionedResps extends VersionedResponses
    ? MappedVersionedResps
    : VersionedResponses
  : VersionedResponses;

export type MapSessionSchema<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>
> = SessionSchema extends infer UnmappedSessionSchema
  ? UnmappedSessionSchema extends SessionObject<SV>
    ? MapSchema<SV, UnmappedSessionSchema>
    : never
  : never;

/**
 * Represents a schema middleware handler with typed parameters, responses, body, and query.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template P - A type for parameter schemas, defaulting to ParamsObject.
 * @template ResBodyMap - A type for response schemas, defaulting to ResponsesObject.
 * @template ReqBody - A type for the request body, defaulting to Body.
 * @template ReqQuery - A type for the request query, defaulting to QueryObject.
 * @template LocalsObj - A type for local variables, defaulting to an empty object.
 */
export type ExpressLikeSchemaHandler<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends Body<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  LocalsObj extends Record<string, unknown>,
  VersionedApi extends VersionSchema<SV, Method>,
  SessionSchema extends Record<string, unknown>,
  BaseRequest,
  BaseResponse,
  NextFunction
> = ExpressLikeHandler<
  SV,
  MapParamsSchema<SV, P>,
  MapResBodyMapSchema<SV, ResBodyMap>,
  MapReqBodySchema<SV, ReqBody>,
  MapReqQuerySchema<SV, ReqQuery>,
  MapReqHeadersSchema<SV, ReqHeaders>,
  MapResHeadersSchema<SV, ResHeaders>,
  LocalsObj,
  MapVersionedReqsSchema<SV, VersionedApi>,
  MapVersionedRespsSchema<SV, VersionedApi>,
  MapSessionSchema<SV, SessionSchema>,
  BaseRequest,
  BaseResponse,
  NextFunction
>;

/**
 * Represents a function that maps an authenticated request with schema validation
 * to a set of authorization strings, with request properties automatically inferred from the schema.
 *
 * @template SV - The type representing the schema validator.
 * @template P - The type representing request parameters inferred from the schema.
 * @template ReqBody - The type representing the request body inferred from the schema.
 * @template ReqQuery - The type representing the request query parameters inferred from the schema.
 * @template ReqHeaders - The type representing the request headers inferred from the schema.
 *
 * @param {ForklaunchRequest<SV, P, ReqBody, ReqQuery, ReqHeaders>} req - The request object with schema validation.
 * @returns {Set<string> | Promise<Set<string>>} - A set of authorization strings or a promise that resolves to it.
 */
export type ExpressLikeSchemaAuthMapper<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>,
  ReqBody extends Body<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  VersionedReqs extends VersionSchema<SV, Method>,
  SessionSchema extends SessionObject<SV>,
  BaseRequest
> = ExpressLikeAuthMapper<
  SV,
  P extends infer UnmappedParams
    ? UnmappedParams extends ParamsObject<SV>
      ? MapParamsSchema<SV, UnmappedParams>
      : never
    : never,
  ReqBody extends infer UnmappedReqBody
    ? UnmappedReqBody extends Body<SV>
      ? MapReqBodySchema<SV, UnmappedReqBody>
      : never
    : never,
  ReqQuery extends infer UnmappedReqQuery
    ? UnmappedReqQuery extends QueryObject<SV>
      ? MapReqQuerySchema<SV, UnmappedReqQuery>
      : never
    : never,
  ReqHeaders extends infer UnmappedReqHeaders
    ? UnmappedReqHeaders extends HeadersObject<SV>
      ? MapReqHeadersSchema<SV, UnmappedReqHeaders>
      : never
    : never,
  VersionedReqs extends infer UnmappedVersionedReqs
    ? UnmappedVersionedReqs extends VersionSchema<SV, Method>
      ? MapVersionedReqsSchema<SV, UnmappedVersionedReqs>
      : never
    : never,
  SessionSchema extends infer UnmappedSessionSchema
    ? UnmappedSessionSchema extends Record<string, unknown>
      ? MapSessionSchema<SV, UnmappedSessionSchema>
      : never
    : never,
  BaseRequest
>;

export type ExpressLikeAuthMapper<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends Record<string, unknown>,
  ReqHeaders extends Record<string, string>,
  VersionedReqs extends VersionedRequests,
  SessionSchema extends Record<string, unknown>,
  BaseRequest
> = (
  payload: JWTPayload & SessionSchema,
  req?: ResolvedForklaunchRequest<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    VersionedReqs,
    SessionSchema,
    BaseRequest
  >
) => Set<string> | Promise<Set<string>>;

type TokenPrefix<Auth extends AuthMethodsBase> =
  undefined extends Auth['tokenPrefix']
    ? Auth extends BasicAuthMethods
      ? 'Basic '
      : Auth extends HmacMethods
        ? 'HMAC '
        : 'Bearer '
    : `${Auth['tokenPrefix']} `;

type AuthHeaders<Auth extends AuthMethodsBase> =
  undefined extends Auth['headerName']
    ? {
        authorization: Auth extends HmacMethods
          ? `${TokenPrefix<Auth>}keyId=${string} ts=${string} nonce=${string} signature=${string}`
          : `${TokenPrefix<Auth>}${string}`;
      }
    : {
        [K in NonNullable<Auth['headerName']>]: `${TokenPrefix<Auth>}${string}`;
      };

type AuthCollapse<Auth extends AuthMethodsBase> = undefined extends Auth['jwt']
  ? undefined extends Auth['basic']
    ? undefined extends Auth['hmac']
      ? true
      : false
    : false
  : false;

export type LiveTypeFunctionRequestInit<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>,
  ReqBody extends Body<SV> | undefined,
  ReqQuery extends QueryObject<SV> | undefined,
  ReqHeaders extends HeadersObject<SV> | undefined,
  Auth extends AuthMethodsBase
> = MakePropertyOptionalIfChildrenOptional<
  (Body<SV> extends ReqBody
    ? unknown
    : {
        body: MapSchema<SV, ReqBody>;
      }) &
    (QueryObject<SV> extends ReqQuery
      ? unknown
      : {
          query: MapSchema<SV, ReqQuery>;
        }) &
    (HeadersObject<SV> extends ReqHeaders
      ? true extends AuthCollapse<Auth>
        ? unknown
        : {
            headers: AuthHeaders<Auth>;
          }
      : true extends AuthCollapse<Auth>
        ? { headers: MapSchema<SV, ReqHeaders> }
        : {
            headers: MapSchema<SV, ReqHeaders> & AuthHeaders<Auth>;
          }) &
    (ParamsObject<SV> extends P
      ? unknown
      : {
          params: MapSchema<SV, P>;
        })
>;

/**
 * Represents a live type function for the SDK.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template Path - A type for the route path.
 * @template P - A type for request parameters.
 * @template ResBodyMap - A type for response schemas.
 * @template ReqBody - A type for the request body.
 * @template ReqQuery - A type for the request query.
 * @template ReqHeaders - A type for the request headers.
 * @template ResHeaders - A type for the response headers.
 *
 */
export type LiveTypeFunction<
  SV extends AnySchemaValidator,
  Route extends string,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends Body<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  ContractMethod extends Method,
  VersionedApi extends VersionSchema<SV, ContractMethod>,
  Auth extends AuthMethodsBase
> = string extends keyof VersionedApi
  ? (
      route: SanitizePathSlashes<Route>,
      ...reqInit: Prettify<
        Omit<
          RequestInit,
          'method' | 'body' | 'query' | 'headers' | 'params'
        > & {
          method: Uppercase<ContractMethod>;
        } & LiveTypeFunctionRequestInit<
            SV,
            P,
            ReqBody,
            ReqQuery,
            ReqHeaders,
            Auth
          >
      > extends infer ReqInit
        ? ReqInit extends
            | { body: unknown }
            | { params: unknown }
            | { query: unknown }
            | { headers: unknown }
          ? [reqInit: Prettify<ReqInit>]
          : [reqInit?: Prettify<ReqInit>]
        : never
    ) => Promise<
      Prettify<
        SdkResponse<
          SV,
          ResponsesObject<SV> extends ResBodyMap
            ? Record<number, unknown>
            : ResBodyMap,
          ForklaunchResHeaders extends ResHeaders
            ? unknown
            : MapSchema<SV, ResHeaders>
        >
      >
    >
  : {
      [K in keyof VersionedApi]: (
        ...reqInit: Prettify<
          Omit<
            RequestInit,
            'method' | 'body' | 'query' | 'headers' | 'params'
          > &
            LiveTypeFunctionRequestInit<
              SV,
              P,
              VersionedApi[K]['body'] extends Body<SV>
                ? VersionedApi[K]['body']
                : Body<SV>,
              VersionedApi[K]['query'] extends QueryObject<SV>
                ? VersionedApi[K]['query']
                : QueryObject<SV>,
              VersionedApi[K]['requestHeaders'] extends HeadersObject<SV>
                ? VersionedApi[K]['requestHeaders']
                : HeadersObject<SV>,
              Auth
            >
        > & { version: K } extends infer ReqInit
          ? ReqInit extends
              | { body: unknown }
              | { params: unknown }
              | { query: unknown }
              | { headers: unknown }
            ? [reqInit: Prettify<ReqInit>]
            : [reqInit?: Prettify<ReqInit>]
          : never
      ) => Promise<
        Prettify<
          SdkResponse<
            SV,
            ResponsesObject<SV> extends VersionedApi[K]['responses']
              ? Record<number, unknown>
              : VersionedApi[K]['responses'],
            ForklaunchResHeaders extends VersionedApi[K]['responseHeaders']
              ? unknown
              : MapSchema<SV, VersionedApi[K]['responseHeaders']>
          >
        >
      >;
    };

/**
 * Represents a live type function for the SDK.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template P - A type for request parameters.
 * @template ResBodyMap - A type for response schemas.
 * @template ReqBody - A type for the request body.
 * @template ReqQuery - A type for the request query.
 * @template ReqHeaders - A type for the request headers.
 * @template ResHeaders - A type for the response headers.
 *
 */
export type LiveSdkFunction<
  SV extends AnySchemaValidator,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends Body<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  VersionedApi extends VersionSchema<SV, Method>,
  Auth extends AuthMethodsBase
> = string extends keyof VersionedApi
  ? (
      ...reqInit: Prettify<
        Omit<RequestInit, 'method' | 'body' | 'query' | 'headers' | 'params'> &
          LiveTypeFunctionRequestInit<
            SV,
            P,
            ReqBody,
            ReqQuery,
            ReqHeaders,
            Auth
          >
      > extends infer ReqInit
        ? ReqInit extends
            | { body: unknown }
            | { params: unknown }
            | { query: unknown }
            | { headers: unknown }
          ? [reqInit: Prettify<ReqInit>]
          : [reqInit?: Prettify<ReqInit>]
        : never
    ) => Promise<
      Prettify<
        SdkResponse<
          SV,
          ResponsesObject<SV> extends ResBodyMap
            ? Record<number, unknown>
            : ResBodyMap,
          ForklaunchResHeaders extends ResHeaders
            ? unknown
            : MapSchema<SV, ResHeaders>
        >
      >
    >
  : {
      [K in keyof VersionedApi]: (
        ...reqInit: Prettify<
          Omit<
            RequestInit,
            'method' | 'body' | 'query' | 'headers' | 'params'
          > &
            LiveTypeFunctionRequestInit<
              SV,
              P,
              VersionedApi[K]['body'] extends Body<SV>
                ? VersionedApi[K]['body']
                : Body<SV>,
              VersionedApi[K]['query'] extends QueryObject<SV>
                ? VersionedApi[K]['query']
                : QueryObject<SV>,
              VersionedApi[K]['requestHeaders'] extends HeadersObject<SV>
                ? VersionedApi[K]['requestHeaders']
                : HeadersObject<SV>,
              Auth
            >
        > extends infer ReqInit
          ? ReqInit extends
              | { body: unknown }
              | { params: unknown }
              | { query: unknown }
              | { headers: unknown }
            ? [reqInit: Prettify<ReqInit>]
            : [reqInit?: Prettify<ReqInit>]
          : never
      ) => Promise<
        Prettify<
          SdkResponse<
            SV,
            ResponsesObject<SV> extends VersionedApi[K]['responses']
              ? Record<number, unknown>
              : VersionedApi[K]['responses'],
            ForklaunchResHeaders extends VersionedApi[K]['responseHeaders']
              ? unknown
              : MapSchema<SV, VersionedApi[K]['responseHeaders']>
          >
        >
      >;
    };

/**
 * Represents a basic SDK Response object.
 *
 * @template ResBodyMap - A type for the response body.
 * @template ResHeaders - A type for the response headers.
 */
type SdkResponse<
  SV extends AnySchemaValidator,
  ResBodyMap extends Record<number, unknown>,
  ResHeaders extends Record<string, unknown> | unknown
> = ({
  [K in keyof Omit<ForklaunchResErrors, keyof ResBodyMap>]: {
    code: K;
    contentType: 'text/plain';
    response: ForklaunchResErrors[K];
  };
} & {
  [K in keyof ResBodyMap]: {
    code: K;
    contentType: ExtractContentType<SV, ResBodyMap[K]>;
    response: ExtractResponseBody<SV, ResBodyMap[K]>;
  } & (unknown extends ResHeaders ? unknown : { headers: ResHeaders });
})[keyof (Omit<ForklaunchResErrors, keyof ResBodyMap> & ResBodyMap)];

/**
 * Represents the default error types for responses.
 */
export type ForklaunchResErrors<
  BadRequest = string,
  Unauthorized = string,
  NotFound = string,
  Forbidden = string,
  InternalServerErrorType = string
> = {
  400: BadRequest;
  401: Unauthorized;
  404: NotFound;
  403: Forbidden;
  500: InternalServerErrorType;
};

/**
 * Represents the default header types for responses.
 */
export type ForklaunchResHeaders = { 'x-correlation-id': string };

/**
 * Represents the default error types for responses.
 */
export type ErrorContainer<Code extends number> = {
  /** The error code */
  code: Code;
  /** The error message */
  error: string;
};

/**
 * Represents a parsed response shape.
 */
export type ResponseShape<Params, Headers, Query, Body> = {
  params: Params;
  headers: Headers;
  query: Query;
  body: Body;
};

/**
 * Represents a path match.
 */
export type PathMatch<
  SuppliedPath extends `/${string}`,
  ActualPath extends `/${string}`
> = ActualPath extends SuppliedPath
  ? SuppliedPath extends ActualPath
    ? SuppliedPath
    : never
  : never;
