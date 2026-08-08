import { safeStringify } from '@forklaunch/common';
import { SchemaObject } from 'openapi3-ts/oas31';
import { SchemaValidator } from '../../index';
import {
  LiteralSchema,
  ParseResult
} from '../../src/shared/types/schema.types';

/**
 * Creates a union type string from an array of string literals.
 *
 * @template T - Array of string literals
 * @example
 * type Result = RecursiveUnion<['a', 'b', 'c']>; // 'a | b | c'
 */
type RecursiveUnion<T extends readonly string[]> = T extends readonly [
  infer F extends string,
  ...infer R extends readonly string[]
]
  ? R extends []
    ? F
    : `${F} | ${RecursiveUnion<R>}`
  : '';

type SerializeStringArray<T extends string[]> = T extends [
  infer F extends string,
  ...infer R extends string[]
]
  ? R extends []
    ? F
    : `${F}, ${SerializeStringArray<R>}`
  : '';

/**
 * Function type for creating a typed mock schema.
 *
 * Named (rather than written inline as the first type argument) so declaration
 * emit does not produce `SchemaValidator<<T>...`, which the TypeScript 7
 * native compiler fails to re-parse.
 */
type MockTypeFunction = <T>() => T;

/**
 * A mock implementation of SchemaValidator for testing purposes.
 * This validator represents schemas as strings and provides simple string-based operations.
 */
export class MockSchemaValidator implements SchemaValidator<
  MockTypeFunction,
  <T extends string>(schema: T) => T,
  <T extends string>(schema: T) => T,
  <T extends string>(schema: T) => `optional ${T}`,
  <T extends string>(schema: T) => `array ${T}`,
  <T extends readonly string[]>(schemas: T) => RecursiveUnion<T>,
  <T extends LiteralSchema>(schema: T) => `literal ${T}`,
  <T extends LiteralSchema>(schemaEnum: Record<string, T>) => `enum ${T}`,
  <Args extends string[], ReturnType extends string>(
    args: Args,
    returnType: ReturnType
  ) => `function(${SerializeStringArray<Args>}) => ${ReturnType}`,
  <Key extends string, Value extends string>(
    key: Key,
    value: Value
  ) => `{${Key}: ${Value}}`,
  <T extends string>(schema: T) => `Promise<${T}>`,
  (value: unknown) => value is string,
  <T extends string>(value: unknown, type: T) => value is T,
  <T extends string>(schema: T, value: string) => boolean,
  <T extends string>(schema: T, value: string) => ParseResult<T>,
  <T extends string>(schema: T) => SchemaObject
> {
  _Type = 'Mock' as const;
  _SchemaCatchall!: string;
  _ValidSchemaObject!: string;

  string = 'string';
  uuid = 'uuid';
  email = 'email';
  uri = 'uri';
  number = 'number';
  bigint = 'bigint';
  boolean = 'boolean';
  date = 'date';
  symbol = 'symbol';
  nullish = 'nullish';
  any = 'any';
  unknown = 'unknown';
  never = 'never';
  binary = 'binary';
  file = 'file';

  type<T>() {
    return 'type' as T;
  }

  /**
   * Compiles a schema string.
   *
   * @param {T} schema - The schema string to compile
   * @returns {T} The same schema string
   */
  compile<T extends string>(schema: T): T {
    return schema;
  }

  /**
   * Converts a schema string to its schemified form.
   *
   * @param {T} schema - The schema string to schemify
   * @returns {T} The same schema string
   */
  schemify<T extends string>(schema: T) {
    return schema;
  }

  /**
   * Makes a schema string optional.
   *
   * @param {T} schema - The schema string to make optional
   * @returns {`optional ${T}`} The schema string prefixed with 'optional'
   */
  optional<T extends string>(schema: T): `optional ${T}` {
    return `optional ${schema}` as `optional ${T}`;
  }

  /**
   * Creates an array schema string.
   *
   * @param {T} schema - The schema string to convert to an array
   * @returns {`array ${T}`} The schema string prefixed with 'array'
   */
  array<T extends string>(schema: T): `array ${T}` {
    return `array ${schema}` as `array ${T}`;
  }

  /**
   * Creates a union schema string from multiple schema strings.
   *
   * @param {T} schemas - Array of schema strings to union
   * @returns {RecursiveUnion<T>} The schema strings joined with ' | '
   */
  union<T extends readonly string[]>(schemas: T): RecursiveUnion<T> {
    return schemas.join(' | ') as RecursiveUnion<T>;
  }

  /**
   * Creates a literal schema string.
   *
   * @param {T} schema - The literal value
   * @returns {`literal ${T}`} The schema string prefixed with 'literal'
   */
  literal<T extends LiteralSchema>(schema: T): `literal ${T}` {
    return `literal ${schema}`;
  }

  /**
   * Creates an enum schema string.
   *
   * @param {Record<string, T>} schemaEnum - The enum values
   * @returns {`enum ${T}`} The schema string prefixed with 'enum'
   */
  enum_<T extends LiteralSchema>(schemaEnum: Record<string, T>): `enum ${T}` {
    return `enum ${Object.values(schemaEnum).join(' | ')}` as `enum ${T}`;
  }

  /**
   * Creates a function schema string.
   *
   * @param {Args} args - The arguments of the function
   * @param {ReturnType} returnType - The return type of the function
   * @returns {`function(${SerializeStringArray<Args>}) => ${ReturnType}`} The schema string prefixed with 'function'
   */
  function_<Args extends string[], ReturnType extends string>(
    args: Args,
    returnType: ReturnType
  ): `function(${SerializeStringArray<Args>}) => ${ReturnType}` {
    return `function(${args.join(
      ', '
    )}) => ${returnType}` as `function(${SerializeStringArray<Args>}) => ${ReturnType}`;
  }

  /**
   * Creates a record schema string.
   *
   * @param {Key} key - The key schema string
   * @param {Value} value - The value schema string
   * @returns {`{${Key}: ${Value}}`} The schema string prefixed with 'record'
   */
  record<Key extends string, Value extends string>(
    key: Key,
    value: Value
  ): `{${Key}: ${Value}}` {
    return `{${key}: ${value}}` as `{${Key}: ${Value}}`;
  }

  /**
   * Creates a promise schema string.
   *
   * @param {T} schema - The schema string to convert to a promise
   * @returns {`Promise<${T}>`} The schema string prefixed with 'Promise'
   */
  promise<T extends string>(schema: T): `Promise<${T}>` {
    return `Promise<${schema}>` as `Promise<${T}>`;
  }

  /**
   * Checks if a value is a schema string.
   *
   * @param {unknown} value - The value to check
   * @returns {boolean} True if the value is a string
   */
  isSchema(value: unknown): value is string {
    return typeof value === 'string';
  }

  /**
   * Checks if a value is an instance of a schema string.
   *
   * @param {unknown} value - The value to check
   * @param {string} type - The schema string to check against
   * @returns {boolean} True if the value is an instance of the schema string
   */
  isInstanceOf<T extends string>(value: unknown, type: T): value is T {
    return typeof type === 'string' && value === type;
  }

  /**
   * Validates a value against a schema string.
   *
   * @param {T} schema - The schema string to validate against
   * @param {string} value - The value to validate
   * @returns {boolean} True if the schema and value strings match
   */
  validate<T extends string>(schema: T, value: string): boolean {
    return schema === value;
  }

  /**
   * Parses a value against a schema string.
   *
   * @param {T} schema - The schema string to parse against
   * @param {string} value - The value to parse
   * @returns {ParseResult<T>} Success if the schema and value strings match, error otherwise
   */
  parse<T extends string>(schema: T, value: string): ParseResult<T> {
    return safeStringify(schema) === safeStringify(value)
      ? {
          ok: true,
          value: schema
        }
      : {
          ok: false,
          errors: [{ path: [], message: 'Some error' }]
        };
  }

  /**
   * Generates a mock OpenAPI schema object.
   *
   * @returns {SchemaObject} A simple string type schema object
   */
  openapi(): SchemaObject {
    return {
      type: 'string'
    };
  }
}

export const mockSchemaValidator = new MockSchemaValidator();
export const string = mockSchemaValidator.string;
export const number = mockSchemaValidator.number;
export const bigint = mockSchemaValidator.bigint;
export const boolean = mockSchemaValidator.boolean;
export const date = mockSchemaValidator.date;
export const symbol = mockSchemaValidator.symbol;
export const nullish = mockSchemaValidator.nullish;
export const any = mockSchemaValidator.any;
export const unknown = mockSchemaValidator.unknown;
export const never = mockSchemaValidator.never;
export const schemify = mockSchemaValidator.schemify.bind(mockSchemaValidator);
export const optional = mockSchemaValidator.optional.bind(mockSchemaValidator);
export const array = mockSchemaValidator.array.bind(mockSchemaValidator);
// note, use 'as const' when calling on the input array, for proper type parsing
export const union = mockSchemaValidator.union.bind(mockSchemaValidator);
export const literal = mockSchemaValidator.literal.bind(mockSchemaValidator);
export const enum_ = mockSchemaValidator.enum_.bind(mockSchemaValidator);
export const function_ =
  mockSchemaValidator.function_.bind(mockSchemaValidator);
export const record = mockSchemaValidator.record.bind(mockSchemaValidator);
export const promise = mockSchemaValidator.promise.bind(mockSchemaValidator);
export const validate = mockSchemaValidator.validate.bind(mockSchemaValidator);
export const openapi = mockSchemaValidator.openapi.bind(mockSchemaValidator);
