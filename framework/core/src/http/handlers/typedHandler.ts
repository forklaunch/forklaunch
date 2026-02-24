import { AnySchemaValidator } from '@forklaunch/validator';
import { isPath } from '../guards/isPath';
import { ExpressLikeSchemaHandler } from '../types/apiDefinition.types';
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
} from '../types/contractDetails.types';
import { ExpressLikeTypedHandler } from '../types/typedHandler.types';

/**
 * Router class that sets up routes and middleware for an Express router, for use with controller/routes pattern.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template contractDetails - The contract details.
 * @template handlers - The handler middlware and handler.
 */
export function typedHandler<
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
  const VersionedApi extends VersionSchema<SV, ContractMethod>,
  BaseRequest,
  BaseResponse,
  NextFunction,
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
  _schemaValidator: SV,
  _path: Path | undefined,
  _contractMethod: ContractMethod,
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
  ...handlers: ExpressLikeSchemaHandler<
    SV,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedApi,
    ResolvedSessionObject<SV, Auth, SessionObject<SV>>,
    BaseRequest,
    BaseResponse,
    NextFunction
  >[]
): ExpressLikeTypedHandler<
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
export function typedHandler<
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
  const VersionedApi extends VersionSchema<SV, ContractMethod>,
  BaseRequest,
  BaseResponse,
  NextFunction,
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
  _schemaValidator: SV,
  _contractMethod: ContractMethod,
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
  ...handlers: ExpressLikeSchemaHandler<
    SV,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedApi,
    ResolvedSessionObject<SV, Auth, SessionObject<SV>>,
    BaseRequest,
    BaseResponse,
    NextFunction
  >[]
): ExpressLikeTypedHandler<
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
export function typedHandler<
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
  const VersionedApi extends VersionSchema<SV, ContractMethod>,
  BaseRequest,
  BaseResponse,
  NextFunction,
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
  _schemaValidator: SV,
  pathOrContractMethod: Path | ContractMethod,
  contractMethodOrContractDetails:
    | ContractMethod
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
      >,
  contractDetailsOrHandler:
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
        ResolvedSessionObject<SV, Auth, SessionObject<SV>>,
        BaseRequest,
        BaseResponse,
        NextFunction
      >,
  ...handlerArray: ExpressLikeSchemaHandler<
    SV,
    P,
    ResBodyMap,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    ResHeaders,
    LocalsObj,
    VersionedApi,
    ResolvedSessionObject<SV, Auth, SessionObject<SV>>,
    BaseRequest,
    BaseResponse,
    NextFunction
  >[]
): ExpressLikeTypedHandler<
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
> {
  // TODO: Clean this up with guards

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
    ResolvedSessionObject<SV, Auth, SessionObject<SV>>,
    BaseRequest,
    BaseResponse,
    NextFunction
  >[];
  if (typeof contractMethodOrContractDetails === 'string') {
    if (typeof contractDetailsOrHandler !== 'function') {
      contractDetails = contractDetailsOrHandler;
    } else {
      throw new Error('Invalid definition for contract details');
    }
    handlers = handlerArray;
  } else {
    contractDetails = contractMethodOrContractDetails;
    if (typeof contractDetailsOrHandler === 'function') {
      handlers = [contractDetailsOrHandler, ...handlerArray];
    } else {
      throw new Error('Invalid definition for handler');
    }
  }

  if (
    isPath(pathOrContractMethod) &&
    typeof contractMethodOrContractDetails !== 'string'
  ) {
    throw new Error('Contract method not supplied, bailing');
  }

  return {
    _typedHandler: true as const,
    _path: isPath(pathOrContractMethod) ? pathOrContractMethod : undefined,
    _method: isPath(pathOrContractMethod)
      ? (contractMethodOrContractDetails as ContractMethod)
      : pathOrContractMethod,
    contractDetails,
    handlers
  };
}
