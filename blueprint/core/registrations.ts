import {
  forklaunchExpress,
  forklaunchRouter,
  handlers,
  type ExpressApplicationOptions,
  type NextFunction,
  type Request,
  type Response
} from '@forklaunch/express';
import {
  any,
  array,
  bigint,
  binary,
  boolean,
  date,
  email,
  enum_,
  file,
  function_,
  literal,
  never,
  null_,
  nullish,
  number,
  optional,
  promise,
  record,
  SchemaValidator,
  string,
  symbol,
  type,
  undefined_,
  union,
  unknown,
  uri,
  uuid,
  void_
} from '@forklaunch/validator/typebox';

export const schemaValidator = SchemaValidator();
export type SchemaValidator = ReturnType<typeof SchemaValidator>;

export {
  any,
  array,
  bigint,
  binary,
  boolean,
  date,
  email,
  enum_,
  type ExpressApplicationOptions,
  file,
  forklaunchExpress,
  forklaunchRouter,
  function_,
  handlers,
  literal,
  never,
  type NextFunction,
  null_,
  nullish,
  number,
  optional,
  promise,
  record,
  type Request,
  type Response,
  SchemaValidator,
  string,
  symbol,
  type,
  undefined_,
  union,
  unknown,
  uri,
  uuid,
  void_
};

export const IdSchema = {
  id: string
};
export const IdsSchema = {
  ids: array(string)
};
