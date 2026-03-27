import { Prettify } from '@forklaunch/common';
import { SchemaObject } from 'openapi3-ts/oas31';
import { TResolve, TSchemaTranslate } from '../../typebox/types/schema.types';
import { ZodResolve, ZodSchemaTranslate } from '../../zod/types/schema.types';

/**
 * Represents an error with a path and message.
 */
export type ParseError = {
  path: string[];
  message: string;
};

/**
 * The result associated with an attempted parsing.
 *
 */
export type ParseResult<T> =
  | {
      ok: true;
      value: T;
    }
  | {
      ok: false;
      errors?: ParseError[];
    };

/**
 * Interface representing a schema validator.
 *
 * @template SchematicFunction - The function type for schemifying a schema.
 * @template OptionalFunction - The function type for making a schema optional.
 * @template ArrayFunction - The function type for converting a schema into an array.
 * @template UnionFunction - The function type for unionizing multiple schemas.
 * @template LiteralFunction - The function type for creating a literal schema.
 * @template ValidationFunction - The function type for validating a value against a schema.
 * @template ParseFunction - The function type for parsing a value against a schema.
 * @template OpenAPIFunction - The function type for converting a schema into an OpenAPI schema object.
 */
export interface SchemaValidator<
  TypeFunction = <T>() => T,
  CompilationFunction = <T>(schema: T) => unknown,
  SchematicFunction = <T>(schema: T) => unknown,
  OptionalFunction = <T>(schema: T) => unknown,
  ArrayFunction = <T>(schema: T) => unknown,
  UnionFunction = <T>(schemas: T[]) => unknown,
  LiteralFunction = <T extends LiteralSchema>(schema: T) => unknown,
  EnumFunction = <T extends LiteralSchema>(
    schemaEnum: Record<string, T>
  ) => unknown,
  FunctionFunction = <Args, ReturnType>(
    args: Args,
    returnType: ReturnType
  ) => unknown,
  RecordFunction = <Key, Value>(key: Key, value: Value) => unknown,
  PromiseFunction = <T>(schema: T) => unknown,
  SchemaGuardFunction = <T>(value: unknown) => value is T,
  InstanceOfFunction = <T>(value: unknown, type: T) => value is T,
  ValidationFunction = <T>(schema: T, value: unknown) => boolean,
  ParseFunction = <T>(
    schema: T,
    value: unknown
  ) => ParseResult<SchemaResolve<T>>,
  OpenAPIFunction = <T>(schema: T) => SchemaObject
> {
  /**
   * The type of the schema validator.
   */
  _Type: unknown;

  /**
   * The catch-all type for the schema. Meant to be used with non-null assertions.
   */
  _SchemaCatchall: unknown;

  /**
   * The valid schema object type. Meant to be used with non-null assertions.
   */
  _ValidSchemaObject: unknown;

  /**
   * Validator for string type.
   */
  string: unknown;

  /**
   * Validator for uuid type.
   */
  uuid: unknown;

  /**
   * Validator for uri type.
   */
  uri: unknown;

  /**
   * Validator for email type.
   */
  email: unknown;

  /**
   * Validator for number type.
   */
  number: unknown;

  /**
   * Validator for bigint type.
   */
  bigint: unknown;

  /**
   * Validator for boolean type.
   */
  boolean: unknown;

  /**
   * Validator for date type.
   */
  date: unknown;

  /**
   * Validator for symbol type.
   */
  symbol: unknown;

  /**
   * Validator for nullish type.
   */
  nullish: unknown;

  /**
   * Validator for any type.
   */
  any: unknown;

  /**
   * Validator for unknown type.
   */
  unknown: unknown;

  /**
   * Validator for never type.
   */
  never: unknown;

  /**
   * Validator for binary type
   */
  binary: unknown;

  /**
   * Validator for file type
   */
  file: unknown;

  /**
   * Validator for type type
   */
  type: TypeFunction;

  /**
   * Compiles schema if this exists, for optimal performance.
   *
   * @param {T} schema - The schema to compile.
   * @returns {unknown} - The compiled schema.
   */
  compile: CompilationFunction;

  /**
   * Converts a valid schema input into a schemified form.
   *
   * @param {T} schema - The schema to schemify.
   * @returns {unknown} - The schemified form of the schema.
   */
  schemify: SchematicFunction;

  /**
   * Converts a schema into an optional schema.
   *
   * @param {T} schema - The schema to make optional.
   * @returns {unknown} - The optional form of the schema.
   */
  optional: OptionalFunction;

  /**
   * Converts a schema into an array schema.
   *
   * @param {T} schema - The schema to convert into an array.
   * @returns {unknown} - The array form of the schema.
   */
  array: ArrayFunction;

  /**
   * Converts multiple schemas into a union schema.
   *
   * @param {T[]} schemas - The schemas to unionize.
   * @returns {unknown} - The union form of the schemas.
   */
  union: UnionFunction;

  /**
   * Creates a literal schema from a value.
   *
   * @param {T} value - The literal value.
   * @returns {unknown} - The literal schema.
   */
  literal: LiteralFunction;

  /**
   * Creates an enum schema from a record of literal values.
   *
   * @param {Record<string, T>} schemaEnum - The enum schema.
   * @returns {unknown} - The enum schema.
   */
  enum_: EnumFunction;

  /**
   * Creates a function schema from a tuple of arguments and a return type.
   *
   * @param {Args} args - The arguments of the function.
   * @param {ReturnType} returnType - The return type of the function.
   * @returns {unknown} - The function schema.
   */
  function_: FunctionFunction;

  /**
   * Creates a promise schema from a schema.
   *
   * @param {T} schema - The schema to use for the promise.
   * @returns {unknown} - The promise schema.
   */
  promise: PromiseFunction;

  /**
   * Creates a record schema from a key and value schema.
   *
   * @param {Key} key - The key schema.
   * @param {Value} value - The value schema.
   * @returns {unknown} - The record schema.
   */
  record: RecordFunction;

  /**
   * Checks if a value is a schema.
   *
   * @param {unknown} value - The value to check.
   * @returns {boolean} - Whether the value is a schema.
   */
  isSchema: SchemaGuardFunction;

  /**
   * Checks if a value is an instance of a schema.
   *
   * @param {object} value - The value to check.
   * @param {TCatchall} type - The schema to check against.
   * @returns {boolean} - Whether the value is an instance of the schema.
   */
  isInstanceOf: InstanceOfFunction;

  /**
   * Validates a value against a schema.
   *
   * @param {T} schema - The schema to validate against.
   * @param {unknown} value - The value to validate.
   * @returns {boolean} - Whether the value is valid according to the schema.
   */
  validate: ValidationFunction;

  /**
   * Parses a value to a schema validation.
   *
   * @param {T} schema - The schema to validate against.
   * @param {unknown} value - The value to validate.
   * @returns {ParseResult} - The discrimintated parsed value if successful, the error if unsuccessful.
   */
  parse: ParseFunction;

  /**
   * Converts a schema into an OpenAPI schema object.
   *
   * @param {T} schema - The schema to convert.
   * @returns {SchemaObject} - The OpenAPI schema object.
   */
  openapi: OpenAPIFunction;
}

/**
 * Type representing any schema validator.
 */
export type AnySchemaValidator = SchemaValidator<
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown,
  unknown
> & {
  /**
   * The type of the schema resolver.
   */
  _Type: keyof SchemaResolve<unknown>;
};

/**
 * Interface representing schema resolution for different validation libraries.
 *
 * @template T - The type of the schema to resolve.
 */
export interface SchemaResolve<T> {
  /**
   * Schema resolution for Zod.
   */
  Zod: ZodResolve<T>;

  /**
   * Schema resolution for TypeBox.
   */
  TypeBox: TResolve<T>;

  /**
   * Mock schema resolution.
   */
  Mock: T;
}

/**
 * Interface representing schema translation for different validation libraries.
 *
 * @template T - The type of the schema to translate.
 */
export interface SchemaTranslate<T> {
  /**
   * Schema translation for Zod.
   */
  Zod: ZodSchemaTranslate<T>;

  /**
   * Schema translation for TypeBox.
   */
  TypeBox: TSchemaTranslate<T>;

  /**
   * Mock schema translation.
   */
  Mock: T;
}

/**
 * Type representing the prettified schema translation.
 *
 * @template T - The type of the schema to translate.
 * @template SV - The type of the schema validator.
 */
type SchemaPrettify<
  T,
  SV extends AnySchemaValidator
> = SchemaTranslate<T>[SV['_Type']] extends infer Schema
  ? Schema extends Blob | Uint8Array
    ? Schema
    : Prettify<Schema>
  : never;

/**
 * Type representing a schema, which can be a valid schema object or an idiomatic schema.
 *
 * @template T - The type of the schema.
 * @template SV - The type of the schema validator.
 */
export type Schema<
  T extends SV['_ValidSchemaObject'] | IdiomaticSchema<SV>,
  SV extends AnySchemaValidator
> = SchemaTranslate<
  SchemaResolve<T>[SV['_Type']]
>[SV['_Type']] extends infer Schema
  ? Schema extends (...args: infer Args) => infer Return
    ? (...args: Args) => Return
    : SchemaPrettify<SchemaResolve<T>[SV['_Type']], SV>
  : never;

/**
 * Represents a schema for an unboxed object where each key can have an idiomatic schema.
 *
 * @template SV - The type of the schema validator.
 */
export type UnboxedObjectSchema<SV extends AnySchemaValidator> = {
  [key: KeyTypes]: IdiomaticSchema<SV>;
};

/**
 * Represents a schema that can be a literal value.
 */
export type LiteralSchema = string | number | boolean;

/**
 * Represents an idiomatic schema which can be an unboxed object schema, a literal schema, or a catch-all type.
 *
 * @template SV - The type of the schema validator.
 */
export type IdiomaticSchema<SV extends AnySchemaValidator> =
  | LiteralSchema
  | SV['_SchemaCatchall']
  | UnboxedObjectSchema<SV>;

/**
 * Tuple-based lookup for incrementing a number type by one. O(1) indexed access
 * instead of O(n) conditional chain.
 */
type IncrementMap = [
  1,
  2,
  3,
  4,
  5,
  6,
  7,
  8,
  9,
  10,
  11,
  12,
  13,
  14,
  15,
  16,
  17,
  18,
  19,
  20,
  21,
  22,
  23,
  24,
  25
];

/**
 * Increments a number type by one, with support up to 24.
 *
 * @template T - The number type to increment.
 */
export type Increment<T extends number> = T extends keyof IncrementMap
  ? IncrementMap[T]
  : 25;

/**
 * Represents key types that can be used in the schema.
 */
export type KeyTypes = string | number;
