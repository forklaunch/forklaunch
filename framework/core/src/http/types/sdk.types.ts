import {
  Prettify,
  TypeSafeFunction,
  UnionToIntersection
} from '@forklaunch/common';
import { AnySchemaValidator } from '@forklaunch/validator';
import { LiveSdkFunction, LiveTypeFunction } from './apiDefinition.types';
import {
  AuthMethodsBase,
  Body,
  HeadersObject,
  Method,
  ParamsObject,
  QueryObject,
  ResponsesObject,
  VersionSchema
} from './contractDetails.types';

/**
 * Creates a type-safe fetch function based on a provided fetch map.
 * This type generates a function that provides compile-time type safety for HTTP requests,
 * ensuring that the correct path and request parameters are used for each endpoint.
 *
 * @template FetchMap - A record mapping paths to their corresponding TypeSafeFunction signatures
 * @param FetchMap[Path] - Each path should map to a TypeSafeFunction that defines the request/response contract
 *
 * @returns A generic function that:
 * - Takes a path parameter constrained to keys of FetchMap
 * - Takes optional request initialization parameters based on the function signature
 * - Returns a Promise of the expected response type
 *
 * @remarks
 * The function parameters are conditionally typed:
 * - If the mapped function expects body, query, params, or headers, reqInit is required
 * - Otherwise, reqInit is optional
 * - If the path doesn't map to a TypeSafeFunction, reqInit is never
 **/
export type FetchFunction<FetchMap> = <
  const Path extends keyof FetchMap,
  const Method extends keyof FetchMap[Path],
  const Version extends keyof FetchMap[Path][Method]
>(
  path: Path,
  ...reqInit: FetchMap[Path][Method] extends TypeSafeFunction
    ? 'GET' extends keyof FetchMap[Path]
      ? FetchMap[Path]['GET'] extends TypeSafeFunction
        ? Parameters<FetchMap[Path]['GET']>[1] extends
            | { body: unknown }
            | { query: unknown }
            | { params: unknown }
            | { headers: unknown }
            | { version: unknown }
          ? [
              reqInit: Omit<Parameters<FetchMap[Path][Method]>[1], 'method'> & {
                method: Method;
              }
            ]
          : [
              reqInit?: Omit<
                Parameters<FetchMap[Path][Method]>[1],
                'method'
              > & {
                method: Method;
              }
            ]
        : [
            reqInit: Omit<Parameters<FetchMap[Path][Method]>[1], 'method'> & {
              method: Method;
            }
          ]
      : [
          reqInit: Omit<Parameters<FetchMap[Path][Method]>[1], 'method'> & {
            method: Method;
          }
        ]
    : FetchMap[Path][Method] extends Record<string, TypeSafeFunction>
      ? [
          reqInit: Omit<
            Parameters<FetchMap[Path][Method][Version]>[0],
            'method' | 'version'
          > & {
            method: Method;
            version: Version;
          }
        ]
      : [{ method: Method }]
) => Promise<
  FetchMap[Path][Method] extends TypeSafeFunction
    ? Awaited<ReturnType<FetchMap[Path][Method]>>
    : FetchMap[Path][Method] extends Record<string, TypeSafeFunction>
      ? Awaited<ReturnType<FetchMap[Path][Method][Version]>>
      : never
>;

/**
 * Represents the structure of a SDK router.
 *
 * @property sdk - The SDK object containing all the SDK functions.
 * @property _fetchMap - The fetch map object containing all the fetch functions.
 * @property sdkPaths - The SDK paths object containing all the SDK paths.
 */
export type SdkRouter = {
  sdk: Record<string, unknown>;
  _fetchMap: Record<string, unknown>;
  sdkPaths: Record<string, string>;
};

/**
 * Recursive interface representing a hierarchical map of router configurations.
 * Each entry can either be a leaf node with SDK and fetch map configurations,
 * or a nested RouterMap for deeper routing structures.
 *
 * @template SV - The schema validator type that constrains the router structure
 * @param SV - Must extend AnySchemaValidator to ensure type safety
 *
 * @example
 * ```typescript
 * const routerMap: RouterMap<ZodValidator> = {
 *   api: {
 *     users: {
 *       sdk: { getUser: () => Promise.resolve({}) },
 *       _fetchMap: { getUser: { get: () => fetch('/api/users') } }
 *     },
 *     posts: {
 *       sdk: { getPosts: () => Promise.resolve([]) },
 *       _fetchMap: { getPosts: { get: () => fetch('/api/posts') } }
 *     }
 *   }
 * };
 * ```
 */
export type RouterMap<SV extends AnySchemaValidator> = {
  [K: string]: SdkRouter | RouterMap<SV>;
};

/**
 * Recursive type representing a hierarchical map of SDK handlers.
 * Each key can either be a leaf node containing a SdkHandler, or a nested SdkHandlerObject for deeper structures.
 *
 * @template SV - The schema validator type that constrains the handler structure.
 *
 * @example
 * ```typescript
 * const handlers: SdkHandlerObject<ZodValidator> = {
 *   users: {
 *     getUser: someSdkHandler,
 *     posts: {
 *       getPosts: anotherSdkHandler
 *     }
 *   }
 * };
 * ```
 */
export type SdkHandlerObject<SV extends AnySchemaValidator> = {
  [K: string]: SdkHandler | SdkHandlerObject<SV>;
};

/**
 * @deprecated
 * Tail-recursive type that extracts and flattens fetch map interfaces from a RouterMap structure.
 * This version uses an accumulator pattern to avoid deep recursion and improve performance.
 * Similar to MapToSdk but focuses on _fetchMap properties and merges all fetch maps into a single intersection type.
 *
 * @template SV - The schema validator type
 * @template T - The RouterMap to extract fetch maps from
 * @template Acc - The accumulator type for collecting fetch maps (defaults to empty object)
 * @param SV - Must extend AnySchemaValidator
 * @param T - Must extend RouterMap<SV>
 * @param Acc - The accumulated fetch maps so far
 *
 * @returns An intersection type containing all fetch map interfaces from the router structure,
 *         flattened into a single type for unified fetch functionality
 *
 * @example
 * ```typescript
 * // Given a RouterMap with nested fetch maps
 * type ExtractedFetch = MapToFetch<ZodValidator, typeof routerMap>;
 * // Results in: { getUser: { get: () => Promise<Response> } } & { getPosts: { get: () => Promise<Response> } }
 * ```
 */
export type MapToFetch<
  SV extends AnySchemaValidator,
  T extends RouterMap<SV>
> = UnionToIntersection<
  Prettify<
    {
      [K in keyof T]: T[K] extends RouterMap<SV>
        ? MapToFetch<SV, T[K]>
        : T[K] extends { _fetchMap: unknown }
          ? T[K]['_fetchMap'] extends Record<string, unknown>
            ? T[K]['_fetchMap']
            : never
          : never;
    }[keyof T]
  >
>;
/**
 * Base interface for controller entries that defines the structure
 * of each controller method with its path, HTTP method, and contract details.
 * This type serves as the foundation for type-safe SDK generation by ensuring
 * all controller entries follow a consistent structure.
 *
 * @example
 * ```typescript
 * const controller: Record<string, SdkHandler> = {
 *   createUser: {
 *     _path: '/users',
 *     _method: 'post',
 *     contractDetails: {
 *       name: 'createUser',
 *       body: { name: string, email: string },
 *       responses: { 201: { id: string, name: string } }
 *     }
 *   }
 * };
 * ```
 */
export type SdkHandler = {
  /** The HTTP path for this endpoint, must start with '/' */
  _path?: `/${string}`;
  /** The HTTP method for this endpoint (get, post, put, etc.) */
  _method?: Method;
  /** Contract details defining the request/response schema for this endpoint */
  contractDetails: {
    /** The name of this endpoint/method */
    name: string;
    /** URL parameters schema */
    params?: unknown;
    /** Response schemas for different status codes */
    responses?: unknown;
    /** Query parameters schema */
    query?: unknown;
    /** Request body schema */
    body?: unknown;
    /** Request headers schema */
    requestHeaders?: unknown;
    /** Response headers schema */
    responseHeaders?: unknown;
    /** Authentication requirements */
    auth?: unknown;
    /** API versioning information */
    versions?: unknown;
  };
};

/**
 * Recursively maps a client controller definition to its corresponding live SDK function types.
 *
 * This utility type traverses the structure of a client controller object, replacing each
 * {@link SdkHandler} with its corresponding {@link MapHandlerToLiveSdk} type, while recursively
 * processing nested objects. This enables type-safe SDK generation for complex controller hierarchies.
 *
 * @template SV - The schema validator type (e.g., zod, typebox).
 * @template Client - The client controller object to map.
 *
 * @example
 * type MySdk = MapToSdk<typeof z, typeof myController>;
 */
export type MapToSdk<SV extends AnySchemaValidator, Client> = {
  [K in keyof Client]: Client[K] extends SdkHandler
    ? MapHandlerToLiveSdk<SV, Client[K]>
    : MapToSdk<SV, Client[K]>;
};

export type MapHandlerToLiveSdk<
  SV extends AnySchemaValidator,
  T extends SdkHandler
> = LiveSdkFunction<
  SV,
  T['contractDetails']['params'] extends infer Params | undefined
    ? Params extends ParamsObject<SV>
      ? Params
      : ParamsObject<SV>
    : ParamsObject<SV>,
  T['contractDetails']['responses'] extends infer Responses | undefined
    ? Responses extends ResponsesObject<SV>
      ? Responses
      : ResponsesObject<SV>
    : ResponsesObject<SV>,
  T['contractDetails']['body'] extends infer B | undefined
    ? B extends Body<SV>
      ? B
      : Body<SV>
    : Body<SV>,
  T['contractDetails']['query'] extends infer Q | undefined
    ? Q extends QueryObject<SV>
      ? Q
      : QueryObject<SV>
    : QueryObject<SV>,
  T['contractDetails']['requestHeaders'] extends
    infer RequestHeaders | undefined
    ? RequestHeaders extends HeadersObject<SV>
      ? RequestHeaders
      : HeadersObject<SV>
    : HeadersObject<SV>,
  T['contractDetails']['responseHeaders'] extends
    infer ResponseHeaders | undefined
    ? ResponseHeaders extends HeadersObject<SV>
      ? ResponseHeaders
      : HeadersObject<SV>
    : HeadersObject<SV>,
  T['contractDetails']['versions'] extends infer Versions | undefined
    ? Versions extends VersionSchema<SV, Method>
      ? Versions
      : VersionSchema<SV, Method>
    : VersionSchema<SV, Method>,
  T['contractDetails']['auth'] extends infer Auth | undefined
    ? Auth extends AuthMethodsBase
      ? Auth
      : AuthMethodsBase
    : AuthMethodsBase
>;
/**
 * @deprecated
 * Extracts and constructs a LiveTypeFunction from an SdkHandler object.
 * This optimized version reduces redundant type inference while maintaining type safety.
 *
 * @template Entry - The controller entry containing path, method, and contract details
 * @template SV - The schema validator type that constrains the contract details
 * @template BasePath - The base path prefix to prepend to the entry's path
 *
 * @example
 * ```typescript
 * type UserCreateFunction = ExtractLiveTypeFn<
 *   { _path: '/users', _method: 'post', contractDetails: { body: { name: string } } },
 *   SchemaValidator,
 *   '/api/v1'
 * >;
 * ```
 */
export type ExtractLiveTypeFn<
  Entry extends SdkHandler,
  SV extends AnySchemaValidator,
  BasePath extends `/${string}`
> = LiveTypeFunction<
  SV,
  Entry['_path'] extends infer Path | undefined
    ? Path extends `/${string}`
      ? `${BasePath}${Path}`
      : never
    : never,
  Entry['contractDetails']['params'] extends infer Params | undefined
    ? Params extends ParamsObject<SV>
      ? Params
      : ParamsObject<SV>
    : ParamsObject<SV>,
  Entry['contractDetails']['responses'] extends infer Responses | undefined
    ? Responses extends ResponsesObject<SV>
      ? Responses
      : ResponsesObject<SV>
    : ResponsesObject<SV>,
  Entry['contractDetails']['body'] extends infer B | undefined
    ? B extends Body<SV>
      ? B
      : Body<SV>
    : Body<SV>,
  Entry['contractDetails']['query'] extends infer Q | undefined
    ? Q extends QueryObject<SV>
      ? Q
      : QueryObject<SV>
    : QueryObject<SV>,
  Entry['contractDetails']['requestHeaders'] extends
    infer RequestHeaders | undefined
    ? RequestHeaders extends HeadersObject<SV>
      ? RequestHeaders
      : HeadersObject<SV>
    : HeadersObject<SV>,
  Entry['contractDetails']['responseHeaders'] extends
    infer ResponseHeaders | undefined
    ? ResponseHeaders extends HeadersObject<SV>
      ? ResponseHeaders
      : HeadersObject<SV>
    : HeadersObject<SV>,
  Entry['_method'] extends Method ? Entry['_method'] : never,
  Entry['contractDetails']['versions'] extends infer Versions | undefined
    ? Versions extends VersionSchema<SV, Method>
      ? Versions
      : VersionSchema<SV, Method>
    : VersionSchema<SV, Method>,
  Entry['contractDetails']['auth'] extends infer Auth | undefined
    ? Auth extends AuthMethodsBase
      ? Auth
      : AuthMethodsBase
    : AuthMethodsBase
>;

/**
 * @deprecated
 * Transforms a controller object into a fetch map structure that provides
 * type-safe access to HTTP endpoints. This optimized version reduces complexity
 * while maintaining full type safety and discriminated union behavior.
 *
 * @template T - The controller object type containing all endpoint definitions
 * @template SV - The schema validator type that constrains the contract details
 * @template RouterBasePath - The base path prefix for the router
 *
 * @example
 * ```typescript
 * const controller = {
 *   createUser: { _path: '/users', _method: 'post', contractDetails: { body: { name: string } } },
 *   getUser: { _path: '/users/:id', _method: 'get', contractDetails: { params: { id: string } } }
 * } as const;
 *
 * type FetchMap = ToFetchMap<typeof controller, SchemaValidator, '/api/v1'>;
 * // Results in properly typed fetch map with discriminated methods per path
 * ```
 */
export type ToFetchMap<
  T extends Record<string, SdkHandler>,
  SV extends AnySchemaValidator,
  RouterBasePath extends `/${string}`
> = Prettify<{
  [
    K in keyof T as T[K]['_path'] extends infer P | undefined
      ? P extends `/${string}`
        ? `${RouterBasePath}${P}`
        : never
      : never
  ]: {
    [
      M in T[K]['_method'] as M extends Method ? Uppercase<M> : never
    ]: ExtractLiveTypeFn<
      Extract<
        T[K],
        {
          _path: T[K]['_path'];
          _method: M;
        }
      >,
      SV,
      RouterBasePath
    >;
  };
}>;
