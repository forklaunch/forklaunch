/**
 * This module provides a Zod-based schema definition.
 * It includes various types, schema creation, validation, and OpenAPI integration.
 *
 * @module ZodSchemaValidator
 */

import { deepCloneWithoutUndefined } from '@forklaunch/common';
import { SchemaObject } from 'openapi3-ts/oas31';
import {
  z,
  ZodArray,
  ZodFunction,
  ZodLiteral,
  ZodObject,
  ZodOptional,
  ZodPromise,
  ZodRawShape,
  ZodRecord,
  ZodTuple,
  ZodType,
  ZodUnion
} from 'zod/v3';
import { extendZodWithOpenApi } from '../../shims/zod-v3-openapi/zod-extensions';
import { generateSchema } from '../../shims/zod-v3-openapi/zod-openapi';
import {
  LiteralSchema,
  ParseResult,
  SchemaValidator as SV
} from '../shared/types/schema.types';
import {
  TupleZodResolve,
  UnionZodResolve,
  ZodCatchall,
  ZodIdiomaticSchema,
  ZodRecordKey,
  ZodResolve,
  ZodSchemaTranslate,
  ZodTupleContainer,
  ZodUnionContainer
} from './types/schema.types';

extendZodWithOpenApi(z);

/**
 * Function type for creating a typed Zod schema.
 *
 * Named (rather than written inline as the first type argument) so declaration
 * emit does not produce `SV<<T>...`, which the TypeScript 7 native compiler
 * fails to re-parse.
 */
type ZodTypeFunction = <T>() => ZodType<T>;

/**
 * Class representing a Zod schema definition.
 * @implements {StaticSchemaValidator}
 */
export class ZodSchemaValidator implements SV<
  ZodTypeFunction,
  <T extends ZodObject<ZodRawShape>>(schema: T) => ZodResolve<T>,
  <T extends ZodIdiomaticSchema>(schema: T) => ZodResolve<T>,
  <T extends ZodIdiomaticSchema>(schema: T) => ZodOptional<ZodResolve<T>>,
  <T extends ZodIdiomaticSchema>(schema: T) => ZodArray<ZodResolve<T>>,
  <T extends ZodUnionContainer>(schemas: T) => ZodUnion<UnionZodResolve<T>>,
  <T extends LiteralSchema>(value: T) => ZodLiteral<T>,
  <T extends Record<string, LiteralSchema>>(
    schemaEnum: T
  ) => ZodUnion<
    [
      {
        [K in keyof T]: ZodLiteral<T[K]>;
      }[keyof T]
    ]
  >,
  <Args extends ZodTupleContainer, ReturnType extends ZodIdiomaticSchema>(
    args: Args,
    returnType: ReturnType
  ) => ZodFunction<
    ZodTuple<TupleZodResolve<Args>, null>,
    ZodResolve<ReturnType>
  >,
  <Key extends ZodIdiomaticSchema, Value extends ZodIdiomaticSchema>(
    key: Key,
    value: Value
  ) => ZodRecord<ZodRecordKey<Key>, ZodResolve<Value>>,
  <T extends ZodIdiomaticSchema>(schema: T) => ZodPromise<ZodResolve<T>>,
  (value: unknown) => value is ZodType,
  <T extends ZodType>(value: object, type: T) => value is T,
  <T extends ZodIdiomaticSchema | ZodCatchall>(
    schema: T,
    value: unknown
  ) => boolean,
  <T extends ZodIdiomaticSchema | ZodCatchall>(
    schema: T,
    value: unknown
  ) => ParseResult<ZodSchemaTranslate<ZodResolve<T>>>,
  <T extends ZodIdiomaticSchema | ZodCatchall>(schema: T) => SchemaObject
> {
  _Type = 'Zod' as const;
  _SchemaCatchall!: ZodType;
  _ValidSchemaObject!:
    ZodObject<ZodRawShape> | ZodArray<ZodObject<ZodRawShape>>;

  string = z.string().openapi({
    title: 'String',
    example: 'a string'
  });
  uuid = z.string().uuid().openapi({
    title: 'UUID',
    format: 'uuid',
    pattern:
      '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$',
    example: 'a8b2c3d4-e5f6-g7h8-i9j0-k1l2m3n4o5p6'
  });
  email = z.string().email().openapi({
    title: 'Email',
    format: 'email',
    pattern: '^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$',
    example: 'a@b.com'
  });
  uri = z.string().url().openapi({
    title: 'URI',
    format: 'uri',
    pattern: '^[a-zA-Z][a-zA-Z\\d+-.]*:[^\\s]*$',
    example: 'https://forklaunch.com'
  });
  number = z
    .preprocess((value) => {
      try {
        return Number(value);
      } catch {
        return value;
      }
    }, z.number())
    .openapi({
      title: 'Number',
      example: 123
    });
  bigint = z
    .preprocess((value) => {
      try {
        if (value instanceof Date) {
          return BigInt(value.getTime());
        }
        switch (typeof value) {
          case 'number':
          case 'string':
            return BigInt(value);
          case 'boolean':
            return BigInt(value ? 1 : 0);
          default:
            return value;
        }
      } catch {
        return value;
      }
    }, z.bigint())
    .openapi({
      title: 'BigInt',
      type: 'integer',
      format: 'int64',
      example: 123n
    });
  boolean = z
    .preprocess((val) => {
      if (typeof val === 'string') {
        if (val.toLowerCase() === 'true') return true;
        if (val.toLowerCase() === 'false') return false;
      }
      return val;
    }, z.boolean())
    .openapi({
      title: 'Boolean',
      example: true
    });
  date = z
    .preprocess((value) => {
      try {
        switch (typeof value) {
          case 'string':
            return new Date(value);
          case 'number':
            return new Date(value);
          default:
            return value;
        }
      } catch {
        return value;
      }
    }, z.date())
    .openapi({
      title: 'Date',
      type: 'string',
      format: 'date-time',
      example: '2025-05-16T21:13:04.123Z'
    });
  symbol = z.symbol().openapi({
    title: 'Symbol',
    example: Symbol('symbol')
  });
  nullish = z.union([z.void(), z.null(), z.undefined()]).openapi({
    title: 'Nullish',
    type: 'null',
    example: null
  });
  void = z.void().openapi({
    title: 'Void',
    type: 'null',
    example: undefined
  });
  null = z.null().openapi({
    title: 'Null',
    type: 'null',
    example: null
  });
  undefined = z.undefined().openapi({
    title: 'Undefined',
    type: 'null',
    example: undefined
  });
  any = z.any().openapi({
    title: 'Any',
    type: 'object',
    example: 'any'
  });
  unknown = z.unknown().openapi({
    title: 'Unknown',
    type: 'object',
    example: 'unknown'
  });
  never = z.never().openapi({
    title: 'Never',
    type: 'null',
    example: 'never'
  });
  binary = z
    .string()
    .transform((val) => new Uint8Array(Buffer.from(val, 'base64')))
    .openapi({
      title: 'Binary',
      type: 'string',
      format: 'binary',
      example: 'a base-64 encodable string'
    });
  file = z
    .union([
      z.instanceof(Buffer),
      z.instanceof(ArrayBuffer),
      z.instanceof(Blob),
      z.string()
    ])
    .transform((val) => {
      if (val instanceof Buffer) {
        return new Blob([val as Buffer<ArrayBuffer>]);
      }
      if (val instanceof ArrayBuffer) {
        return new Blob([val]);
      }
      if (val instanceof Blob) {
        return val;
      }
      if (typeof val === 'string') {
        return new Blob([val]);
      }
      return new Blob([val as Buffer<ArrayBuffer>]);
    })
    .refine((val) => val instanceof Blob, {
      message:
        'Invalid file type: expected Buffer, ArrayBuffer, Blob, or string'
    })
    .openapi({
      title: 'File',
      type: 'string',
      format: 'binary',
      example: 'a base-64 encodable blob or file'
    });
  type = <T>() => this.any as ZodType<T>;

  /**
   * Compiles schema if this exists, for optimal performance.
   *
   * @param {ZodObject<ZodRawShape>} schema - The schema to compile.
   * @returns {ZodResolve<T>} - The compiled schema.
   */
  compile<T extends ZodObject<ZodRawShape>>(schema: T): ZodResolve<T> {
    return schema as ZodResolve<T>;
  }

  /**
   * Convert a schema to a Zod schema.
   * @param {ZodIdiomaticSchema} schema - The schema to convert.
   * @returns {ZodResolve<T>} The resolved schema.
   */
  schemify<T extends ZodIdiomaticSchema>(schema: T): ZodResolve<T> {
    if (
      typeof schema === 'string' ||
      typeof schema === 'number' ||
      typeof schema === 'boolean'
    ) {
      return z.literal(schema) as ZodResolve<T>;
    }

    if (schema instanceof ZodType) {
      return schema as ZodResolve<T>;
    }

    const newSchema: ZodRawShape = {};
    Object.getOwnPropertyNames(schema).forEach((key) => {
      if (schema[key] instanceof ZodType) {
        newSchema[key] = schema[key];
      } else {
        newSchema[key] = this.schemify(schema[key]);
      }
    });

    return z.object(newSchema) as ZodResolve<T>;
  }

  /**
   * Make a schema optional.
   * @param {ZodIdiomaticSchema} schema - The schema to make optional.
   * @returns {ZodOptional<ZodResolve<T>>} The optional schema.
   */
  optional<T extends ZodIdiomaticSchema>(
    schema: T
  ): ZodOptional<ZodResolve<T>> {
    const resolvedSchema = this.schemify(schema);
    return resolvedSchema.optional() as ZodOptional<ZodResolve<T>>;
  }

  /**
   * Create an array schema.
   * @param {ZodIdiomaticSchema} schema - The schema to use for array items.
   * @returns {ZodArray<ZodResolve<T>>} The array schema.
   */
  array<T extends ZodIdiomaticSchema>(schema: T): ZodArray<ZodResolve<T>> {
    const resolvedSchema = this.schemify(schema);
    return resolvedSchema.array() as ZodArray<ZodResolve<T>>;
  }

  /**
   * Create a union schema.
   * @param {ZodUnionContainer} schemas - The schemas to union.
   * @returns {ZodUnion<UnionZodResolve<T>>} The union schema.
   */
  union<T extends ZodUnionContainer>(schemas: T): ZodUnion<UnionZodResolve<T>> {
    const resolvedSchemas = schemas.map((schema) => this.schemify(schema));

    return z.union(
      resolvedSchemas as [ZodType, ZodType, ...ZodType[]]
    ) as ZodUnion<UnionZodResolve<T>>;
  }

  /**
   * Create a literal schema.
   * @param {LiteralSchema} value - The literal value.
   * @returns {ZodLiteral<ZodResolve<T>>} The literal schema.
   */
  literal<T extends LiteralSchema>(value: T): ZodLiteral<T> {
    return z.literal(value) as ZodLiteral<T>;
  }

  /**
   * Create an enum schema.
   * @param {Record<string, LiteralSchema>} schemaEnum - The enum schema.
   * @returns {ZodUnion<UnionZodResolve<[T, T, ...T[]]>>} The enum schema.
   */
  enum_<T extends Record<string, LiteralSchema>>(
    schemaEnum: T
  ): ZodUnion<
    [
      {
        [K in keyof T]: ZodLiteral<T[K]>;
      }[keyof T]
    ]
  > {
    return this.union(
      Object.values(schemaEnum) as unknown as ZodUnionContainer
    ) as unknown as ZodUnion<
      [
        {
          [K in keyof T]: ZodLiteral<T[K]>;
        }[keyof T]
      ]
    >;
  }

  /**
   * Create a function schema.
   * @param {ZodTuple} args - The arguments of the function.
   * @param {ZodAny} returnType - The return type of the function.
   * @returns {ZodFunction<Args, ReturnType>} The function schema.
   */
  function_<
    Args extends ZodTupleContainer,
    ReturnType extends ZodIdiomaticSchema
  >(
    args: Args,
    returnType: ReturnType
  ): ZodFunction<
    ZodTuple<TupleZodResolve<Args>, null>,
    ZodResolve<ReturnType>
  > {
    const schemaArgs = args.map((schema) => this.schemify(schema)) as [
      ZodType,
      ...ZodType[]
    ];
    const schemaReturnType = this.schemify(returnType);
    return z.function(z.tuple(schemaArgs), schemaReturnType) as ZodFunction<
      ZodTuple<TupleZodResolve<Args>, null>,
      ZodResolve<ReturnType>
    >;
  }

  /**
   * Create a record schema.
   * @param {ZodIdiomaticSchema} key - The key schema.
   * @param {ZodIdiomaticSchema} value - The value schema.
   * @returns {ZodRecord<ZodResolve<Key>, ZodResolve<Value>>} The record schema.
   */
  record<Key extends ZodIdiomaticSchema, Value extends ZodIdiomaticSchema>(
    key: Key,
    value: Value
  ): ZodRecord<ZodRecordKey<Key>, ZodResolve<Value>> {
    const keySchema = this.schemify(key);
    const valueSchema = this.schemify(value);
    return z.record(keySchema, valueSchema) as ZodRecord<
      ZodRecordKey<Key>,
      ZodResolve<Value>
    >;
  }

  /**
   * Create a promise schema.
   * @param {ZodIdiomaticSchema} schema - The schema to use for the promise.
   * @returns {ZodPromise<ZodResolve<T>>} The promise schema.
   */
  promise<T extends ZodIdiomaticSchema>(schema: T): ZodPromise<ZodResolve<T>> {
    return z.promise(this.schemify(schema)) as ZodPromise<ZodResolve<T>>;
  }

  /**
   * Checks if a value is a Zod schema.
   * @param {unknown} value - The value to check.
   * @returns {boolean} True if the value is a Zod schema.
   */
  isSchema(value: unknown): value is ZodType {
    return value instanceof ZodType;
  }

  /**
   * Checks if a value is an instance of a Zod schema.
   * @param {object} value - The value to check.
   * @param {ZodType} type - The schema to check against.
   * @returns {boolean} True if the value is an instance of the schema.
   */
  isInstanceOf<T extends ZodType>(value: unknown, type: T): value is T {
    return (
      this.isSchema(value) &&
      (type._def as { typeName: string }).typeName ===
        (value._def as { typeName: string }).typeName
    );
  }

  /**
   * Validate a value against a schema.
   * @param {ZodCatchall} schema - The schema to validate against.
   * @param {unknown} value - The value to validate.
   * @returns {boolean} True if valid, otherwise false.
   */
  validate<T extends ZodIdiomaticSchema | ZodCatchall>(
    schema: T,
    value: unknown
  ): boolean {
    const resolvedSchema = this.schemify(schema);
    return resolvedSchema.safeParse(value).success;
  }

  /**
   * Parses a value to a schema validation.
   *
   * @param {ZodCatchall} schema - The schema to validate against.
   * @param {unknown} value - The value to validate.
   * @returns {ParseResult} - The discrimintated parsed value if successful, the error if unsuccessful.
   */
  parse<T extends ZodIdiomaticSchema | ZodCatchall>(
    schema: T,
    value: unknown
  ): ParseResult<ZodSchemaTranslate<ZodResolve<T>>> {
    const resolvedSchema = this.schemify(schema);
    const result = resolvedSchema.safeParse(value);
    return result.success
      ? { ok: true, value: result.data }
      : {
          ok: false,
          errors: result.error.errors.flatMap((error) => {
            switch (error.code) {
              case 'invalid_union':
                return error.unionErrors.flatMap((unionError, idx) =>
                  unionError.errors.map((e) => ({
                    path: [
                      `Union Schema Variant ${idx}`,
                      ...error.path.map((p) => p.toString()),
                      ...e.path.map((p) => p.toString())
                    ],
                    message: e.message
                  }))
                );
              default:
                return [
                  {
                    path: error.path.map((p) => p.toString()),
                    message: error.message
                  }
                ];
            }
          })
        };
  }

  /**
   * Convert a schema to an OpenAPI schema object.
   * @param {ZodIdiomaticSchema} schema - The schema to convert.
   * @returns {SchemaObject} The OpenAPI schema object.
   */
  openapi<T extends ZodIdiomaticSchema | ZodCatchall>(schema: T): SchemaObject {
    const schemified = this.schemify(schema);
    return generateSchema(deepCloneWithoutUndefined(schemified));
  }
}
