/**
 * This module provides a TypeScript-based schema definition using the TypeBox library.
 * It includes various types, schema creation, validation, and OpenAPI integration.
 *
 * @module TypeboxSchemaValidator
 */

import { deepCloneWithoutUndefined, InMemoryBlob } from '@forklaunch/common';
import {
  FormatRegistry,
  Kind,
  KindGuard,
  TAny,
  TArray,
  TBigInt,
  TBoolean,
  TDate,
  TFunction,
  TLiteral,
  TNever,
  TNull,
  TNumber,
  TOptional,
  TPromise,
  TProperties,
  TRecord,
  TSchema,
  TString,
  TSymbol,
  TTransform,
  TUndefined,
  TUnion,
  TUnknown,
  TUnsafe,
  TVoid,
  Type,
  TypeRegistry
} from '@sinclair/typebox';
import { TypeCheck, TypeCompiler } from '@sinclair/typebox/compiler';
import {
  DefaultErrorFunction,
  SetErrorFunction,
  ValueErrorType
} from '@sinclair/typebox/errors';
import { Value, ValueError } from '@sinclair/typebox/value';
import { SchemaObject } from 'openapi3-ts/oas31';
import {
  LiteralSchema,
  ParseResult,
  SchemaValidator as SV
} from '../shared/types/schema.types';
import {
  SafeTObject,
  TCatchall,
  TIdiomaticSchema,
  TObjectShape,
  TResolve,
  TSchemaTranslate,
  TUnionTupleContainer,
  UnionTupleTResolve
} from './types/schema.types';

FormatRegistry.Set('binary', (value) => typeof value === 'string');

TypeRegistry.Set('Blob', (_schema, value) => value instanceof Blob);
TypeRegistry.Set(
  'ArrayBuffer',
  (_schema, value) => value instanceof ArrayBuffer
);
TypeRegistry.Set('Buffer', (_schema, value) => value instanceof Buffer);

/**
 * Typebox custom error function
 */
SetErrorFunction((params) => {
  switch (params.errorType) {
    case ValueErrorType.Union:
    case ValueErrorType.Array:
    case ValueErrorType.String:
    case ValueErrorType.Number:
      return params.schema.errorType
        ? `Expected ${params.schema.errorType} value${
            params.schema.errorSuffix ? 's' : ''
          }`
        : DefaultErrorFunction(params);
    default:
      return DefaultErrorFunction(params);
  }
});

/**
 * Function type for creating a typed TypeBox schema.
 *
 * Named (rather than written inline as the first type argument) so declaration
 * emit does not produce `SV<<T>...`, which the TypeScript 7 native compiler
 * fails to re-parse.
 */
type TypeboxTypeFunction = <T>() => TTransform<TAny, T>;

/**
 * Class representing a TypeBox schema definition.
 * @implements {SchemaValidator}
 */
export class TypeboxSchemaValidator implements SV<
  TypeboxTypeFunction,
  <T extends SafeTObject<TProperties>>(schema: T) => TypeCheck<T>,
  <T extends TIdiomaticSchema>(schema: T) => TResolve<T>,
  <T extends TIdiomaticSchema>(schema: T) => TOptional<TResolve<T>>,
  <T extends TIdiomaticSchema>(schema: T) => TArray<TResolve<T>>,
  <T extends TUnionTupleContainer>(
    schemas: [...T]
  ) => TUnion<UnionTupleTResolve<T>>,
  <T extends LiteralSchema>(value: T) => TLiteral<T>,
  <T extends Record<string, LiteralSchema>>(
    schemaEnum: T
  ) => TUnion<
    [
      {
        [K in keyof T]: TLiteral<T[K]>;
      }[keyof T]
    ]
  >,
  <Args extends TUnionTupleContainer, ReturnType extends TIdiomaticSchema>(
    args: [...Args],
    returnType: ReturnType
  ) => TFunction<UnionTupleTResolve<Args>, TResolve<ReturnType>>,
  <Key extends TIdiomaticSchema, Value extends TIdiomaticSchema>(
    key: Key,
    value: Value
  ) => TRecord<TResolve<Key>, TResolve<Value>>,
  <T extends TIdiomaticSchema>(schema: T) => TPromise<TResolve<T>>,
  (value: unknown) => value is TSchema,
  <T extends TCatchall>(value: object, type: T) => value is T,
  <T extends TIdiomaticSchema | TCatchall>(
    schema: T,
    value: unknown
  ) => boolean,
  <T extends TIdiomaticSchema | TCatchall>(
    schema: T,
    value: unknown
  ) => ParseResult<TSchemaTranslate<TResolve<T>>>,
  <T extends TIdiomaticSchema | TCatchall>(schema: T) => SchemaObject
> {
  _Type = 'TypeBox' as const;
  _SchemaCatchall!: TCatchall;
  _ValidSchemaObject!:
    SafeTObject<TProperties> | TArray<SafeTObject<TProperties>>;

  string: TString = Type.String({
    example: 'a string',
    title: 'String'
  });
  uuid: TString = Type.String({
    pattern:
      '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$',
    errorType: 'uuid',
    example: 'a8b2c3d4-e5f6-g7h8-i9j0-k1l2m3n4o5p6',
    title: 'UUID'
  });
  email: TString = Type.String({
    pattern: '^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$',
    errorType: 'email',
    example: 'a@b.com',
    title: 'Email'
  });
  uri: TString = Type.String({
    pattern: '^[a-zA-Z][a-zA-Z\\d+-.]*:[^\\s]*$',
    errorType: 'uri',
    example: 'https://forklaunch.com',
    title: 'URI'
  });
  number: TTransform<
    TUnion<[TNumber, TString, TBoolean, TNull, TBigInt, TDate]>,
    number
  > = Type.Transform(
    Type.Union(
      [
        Type.Number(),
        Type.String({ pattern: '^[0-9]+$' }),
        Type.Boolean(),
        Type.Null(),
        Type.BigInt(),
        Type.Date()
      ],
      {
        errorType: 'number-like',
        example: 123,
        title: 'Number'
      }
    )
  )
    .Decode((value) => {
      if (typeof value !== 'number') {
        const num = Number(value);
        if (isNaN(num)) {
          throw new Error('Invalid number');
        } else {
          return num;
        }
      }
      return value;
    })
    .Encode(Number);
  bigint: TTransform<
    TUnion<[TBigInt, TNumber, TString, TBoolean, TDate]>,
    bigint
  > = Type.Transform(
    Type.Union(
      [
        Type.BigInt(),
        Type.Number(),
        Type.String({ pattern: '^[0-9]+n?$' }),
        Type.Boolean(),
        Type.Date()
      ],
      {
        errorType: 'BigInt-like',
        example: 123n,
        title: 'BigInt'
      }
    )
  )
    .Decode((value) => {
      if (typeof value !== 'bigint') {
        try {
          if (value instanceof Date) {
            return BigInt(value.getTime());
          }
          return BigInt(value);
        } catch {
          throw new Error('Invalid bigint');
        }
      }
      return value;
    })
    .Encode(BigInt);
  boolean: TTransform<TUnion<[TBoolean, TString]>, boolean> = Type.Transform(
    Type.Union(
      [
        Type.Boolean(),
        Type.String({
          pattern: '^(t|T)(r|R)(u|U)(e|E)$|^(f|F)(a|A)(l|L)(s|S)(e|E)$'
        })
      ],
      {
        errorType: 'boolean-like',
        example: true,
        title: 'Boolean'
      }
    )
  )
    .Decode((value) => {
      if (typeof value === 'string') {
        if (value.toLowerCase() === 'true') return true;
        return false;
      } else {
        return value;
      }
    })
    .Encode(Boolean);
  date: TTransform<TUnion<[TString, TNumber, TDate]>, Date> = Type.Transform(
    Type.Union(
      [
        Type.String({
          pattern:
            '^\\d{4}(-\\d{2}){0,2}(T\\d{2}:\\d{2}(:\\d{2}(\\.\\d{1,3})?)?(Z|([+-]\\d{2}:\\d{2}))?)?$|^\\d{1,2}\\/\\d{1,2}\\/\\d{4}$|^\\d{4}\\/\\d{1,2}\\/\\d{1,2}$|^\\d+$'
        }),
        Type.Number(),
        Type.Date()
      ],
      {
        errorType: 'date',
        example: '2025-05-16T21:13:04.123Z',
        title: 'Date'
      }
    )
  )
    .Decode((value) => {
      if (value === null || typeof value === 'boolean') {
        return new Date(value ? 1 : 0);
      }
      return new Date(value);
    })
    .Encode((value) => new Date(value).toISOString());
  symbol: TSymbol = Type.Symbol({
    title: 'Symbol'
  });
  nullish: TUnion<[TVoid, TNull, TUndefined]> = Type.Union(
    [Type.Void(), Type.Null(), Type.Undefined()],
    {
      errorType: 'nullish',
      type: 'null',
      example: 'null',
      title: 'Nullish'
    }
  );
  void: TVoid = Type.Void({
    type: 'null',
    example: 'void',
    title: 'Void'
  });
  null: TNull = Type.Null({
    type: 'null',
    example: 'null',
    title: 'Null'
  });
  undefined: TUndefined = Type.Undefined({
    type: 'null',
    example: 'undefined',
    title: 'Undefined'
  });
  any: TAny = Type.Any({
    type: 'object',
    example: 'any',
    title: 'Any'
  });
  unknown: TUnknown = Type.Unknown({
    type: 'object',
    example: 'unknown',
    title: 'Unknown'
  });
  never: TNever = Type.Never({
    type: 'null',
    example: 'never',
    title: 'Never'
  });
  binary: TTransform<TString, Uint8Array<ArrayBuffer>> = Type.Transform(
    Type.String({
      errorType: 'binary',
      format: 'binary',
      example: 'a base-64 encodable string',
      title: 'Binary'
    })
  )
    .Decode((value) => new Uint8Array(Buffer.from(value, 'base64')))
    .Encode((value) => {
      if (value instanceof Buffer) {
        return String.fromCharCode(...new Uint8Array(value));
      }
      return '';
    });
  file: TTransform<
    TUnion<[TUnsafe<Buffer>, TUnsafe<ArrayBuffer>, TUnsafe<Blob>, TString]>,
    Blob
  > = Type.Transform(
    Type.Union([
      Type.Unsafe<Buffer>({
        [Kind]: 'Buffer',
        errorType: 'binary',
        format: 'binary',
        example: 'a raw buffer or file stream',
        title: 'File'
      }),
      Type.Unsafe<ArrayBuffer>({
        [Kind]: 'ArrayBuffer',
        errorType: 'binary',
        format: 'binary',
        example: 'an array buffer',
        title: 'File'
      }),
      Type.Unsafe<Blob>({
        [Kind]: 'Blob',
        errorType: 'binary',
        format: 'binary',
        example: 'a blob object',
        title: 'File'
      }),
      Type.String({
        errorType: 'binary',
        format: 'binary',
        example: 'a string content',
        title: 'File'
      })
    ])
  )
    .Decode((value) => {
      if (value instanceof Buffer) {
        return new InMemoryBlob(value as Buffer<ArrayBuffer>) as Blob;
      }
      if (value instanceof ArrayBuffer) {
        return new InMemoryBlob(Buffer.from(value)) as Blob;
      }
      if (value instanceof Blob) {
        return value as Blob;
      }
      if (typeof value === 'string') {
        return new InMemoryBlob(Buffer.from(value)) as Blob;
      }
      return new InMemoryBlob(Buffer.from(value)) as Blob;
    })
    .Encode((value) => (value as InMemoryBlob).content);
  type = <T>() => this.any as TTransform<TAny, T>;

  /**
   * Extracts the error type of a schema for error messages.
   *
   * @param {TCatchall} schema - A schema that contains some type information.
   * @returns The type of the schema for error messages.
   */
  private errorType(schema: TCatchall) {
    if (KindGuard.IsSchema(schema) && Object.hasOwn(schema, 'errorType')) {
      return schema.errorType;
    } else if (KindGuard.IsLiteral(schema)) {
      return schema.const;
    }
    return schema[Kind].toLowerCase();
  }

  /**
   * Compiles schema if this exists, for optimal performance.
   *
   * @param {TObject<TProperties>} schema - The schema to compile.
   * @returns {TypeCheck<T>} - The compiled schema.
   */
  compile<T extends SafeTObject<TProperties>>(schema: T): TypeCheck<T> {
    return TypeCompiler.Compile(schema);
  }

  /**
   * Convert a schema to a TypeBox schema.
   * @param {TIdiomaticSchema} schema - The schema to convert.
   * @returns {TResolve<T>} The resolved schema.
   */
  schemify<T extends TIdiomaticSchema>(schema: T): TResolve<T> {
    if (
      typeof schema === 'string' ||
      typeof schema === 'number' ||
      typeof schema === 'boolean'
    ) {
      return Type.Literal(schema) as TResolve<T>;
    }

    if (KindGuard.IsSchema(schema) || schema instanceof TypeCheck) {
      return schema as TResolve<T>;
    }

    const newSchema: TObjectShape = {};
    Object.getOwnPropertyNames(schema).forEach((key) => {
      if (
        KindGuard.IsSchema(schema[key]) ||
        KindGuard.IsTransform(schema[key])
      ) {
        newSchema[key] = schema[key] as TSchema;
      } else {
        const schemified = this.schemify(schema[key]);
        newSchema[key] = schemified;
      }
    });

    return Type.Object(newSchema) as TResolve<T>;
  }

  /**
   * Make a schema optional.
   * @param {TIdiomaticSchema} schema - The schema to make optional.
   * @returns {TOptional<TResolve<T>>} The optional schema.
   */
  optional<T extends TIdiomaticSchema>(schema: T): TOptional<TResolve<T>> {
    const schemified = this.schemify(schema);
    return Type.Optional(schemified) as TOptional<TResolve<T>>;
  }

  /**
   * Create an array schema.
   * @param {TIdiomaticSchema} schema - The schema to use for array items.
   * @returns {TArray<TResolve<T>>} The array schema.
   */
  array<T extends TIdiomaticSchema>(schema: T): TArray<TResolve<T>> {
    const schemified = this.schemify(schema);
    return Type.Array(schemified, {
      errorType: `array of ${this.errorType(schemified)}`
    }) as TArray<TResolve<T>>;
  }

  /**
   * Create a union schema.
   * @param {TUnionTupleContainer} schemas - The schemas to union.
   * @returns {TUnion<UnionTupleTResolve<T>>} The union schema.
   *
   * WARNING: If "nullish" or TUndefined is included in the union, the key will still be expected.
   * This is a limitation of TypeBox. Consider using "optional" instead.
   */
  union<T extends TUnionTupleContainer>(
    schemas: [...T]
  ): TUnion<UnionTupleTResolve<T>> {
    const unionTypes = schemas.map((schema) => {
      return this.schemify(schema);
    });

    return Type.Union(unionTypes, {
      errorType: `any of ${unionTypes
        .map((s) => this.errorType(s))
        .join(', ')}`,
      errorSuffix: true
    }) as TUnion<UnionTupleTResolve<T>>;
  }

  /**
   * Create a literal schema.
   * @param {LiteralSchema} value - The literal value.
   * @returns {TLiteral<T>} The literal schema.
   */
  literal<T extends LiteralSchema>(value: T): TLiteral<T> {
    return Type.Literal(value, {
      errorType: value
    });
  }

  /**
   * Create an enum schema.
   * @param {Record<string, LiteralSchema>} schemaEnum - The enum schema.
   * @returns {TUnion<UnionTupleTResolve<T[]>>} The enum schema.
   */
  enum_<T extends Record<string, LiteralSchema>>(
    schemaEnum: T
  ): TUnion<
    [
      {
        [K in keyof T]: TLiteral<T[K]>;
      }[keyof T]
    ]
  > {
    // The hop through unknown is load-bearing. `union()` is typed for a fixed
    // tuple, and mapping an enum's values produces an array whose length is not
    // known statically, so the two union shapes never overlap enough for a
    // direct assertion. The runtime value is correct — one literal per enum
    // member — but that correspondence is not expressible here.
    return this.union(
      Object.values(schemaEnum).map((value) => this.literal(value))
    ) as unknown as TUnion<
      [
        {
          [K in keyof T]: TLiteral<T[K]>;
        }[keyof T]
      ]
    >;
  }

  /**
   * Create a function schema.
   * @param {TSchema[]} args - The arguments of the function.
   * @param {TAny} returnType - The return type of the function.
   * @returns {TFunction<Args, ReturnType>} The function schema.
   */
  function_<
    Args extends TUnionTupleContainer,
    ReturnType extends TIdiomaticSchema
  >(
    args: [...Args],
    returnType: ReturnType
  ): TFunction<UnionTupleTResolve<Args>, TResolve<ReturnType>> {
    const schemaArgs = args.map((schema) => {
      return this.schemify(schema);
    });
    const schemaReturnType = this.schemify(returnType);
    return Type.Function(schemaArgs, schemaReturnType) as TFunction<
      UnionTupleTResolve<Args>,
      TResolve<ReturnType>
    >;
  }

  /**
   * Create a record schema.
   * @param {TIdiomaticSchema} key - The key schema.
   * @param {TIdiomaticSchema} value - The value schema.
   * @returns {TRecord<TResolve<Key>, TResolve<Value>>} The record schema.
   */
  record<Key extends TIdiomaticSchema, Value extends TIdiomaticSchema>(
    key: Key,
    value: Value
  ): TRecord<TResolve<Key>, TResolve<Value>> {
    const keySchema = this.schemify(key);
    const valueSchema = this.schemify(value);
    return Type.Record(keySchema, valueSchema) as TRecord<
      TResolve<Key>,
      TResolve<Value>
    >;
  }

  /**
   * Create a promise schema.
   * @param {TIdiomaticSchema} schema - The schema to use for the promise.
   * @returns {TPromise<TResolve<T>>} The promise schema.
   */
  promise<T extends TIdiomaticSchema>(schema: T): TPromise<TResolve<T>> {
    return Type.Promise(this.schemify(schema)) as TPromise<TResolve<T>>;
  }

  /**
   * Check if a value is a TypeBox object schema.
   * @param {unknown} value - The value to check.
   * @returns {boolean} True if the value is a TypeBox object schema.
   */
  isSchema(value: unknown): value is TSchema {
    return KindGuard.IsSchema(value);
  }

  /**
   * Check if a value is an instance of a TypeBox schema.
   * @param {object} value - The value to check.
   * @param {TCatchall} type - The schema to check against.
   * @returns {boolean} True if the value is an instance of the schema.
   */
  isInstanceOf<T extends TCatchall>(value: unknown, type: T): value is T {
    return (
      typeof value === 'object' &&
      value != null &&
      Kind in value &&
      value[Kind] === type[Kind]
    );
  }

  /**
   * Validate a value against a schema.
   *
   * @param {TSchema} schema - The schema to validate against.
   * @param {unknown} value - The value to validate.
   * @returns {boolean} True if valid, otherwise false.
   */
  validate<T extends TIdiomaticSchema | TCatchall>(
    schema: T | TypeCheck<TResolve<T>>,
    value: unknown
  ): boolean {
    if (schema instanceof TypeCheck) {
      return schema.Check(value);
    } else {
      const schemified = this.schemify(schema);
      return Value.Check(schemified, value);
    }
  }

  /**
   * Parse a value against a schema.
   *
   * @param {TSchema} schema - The schema to validate against.
   * @param {unknown} value - The value to validate.
   * @returns {ParseResult<TResolve<T>>} The parsing result.
   */
  parse<T extends TIdiomaticSchema | TCatchall>(
    schema: T | TypeCheck<TResolve<T>>,
    value: unknown
  ): ParseResult<TSchemaTranslate<TResolve<T>>> {
    let errors: ValueError[] = [];
    let conversion: unknown;
    if (schema instanceof TypeCheck) {
      if (schema.Check(value)) {
        conversion = schema.Decode(value);
      } else {
        errors = Array.from(schema.Errors(value));
      }
    } else {
      const schemified = this.schemify(schema);

      if (schemified[Kind] === 'Unsafe') {
        try {
          if (value instanceof Buffer) {
            conversion = new InMemoryBlob(value) as Blob;
          } else {
            errors = [
              {
                type: ValueErrorType.String,
                schema: schemified,
                path: '',
                message: `Invalid file type: expected Buffer or string, got ${typeof value}`,
                value: value,
                errors: []
              }
            ];
          }
        } catch (err) {
          errors = [
            {
              type: ValueErrorType.String,
              schema: schemified,
              path: '',
              message: err instanceof Error ? err.message : 'Invalid file type',
              value: value,
              errors: []
            }
          ];
        }
      } else {
        if (Value.Check(schemified, value)) {
          conversion = Value.Decode(schemified, value);
        } else {
          errors = Array.from(Value.Errors(schemified, value));
        }
      }
    }

    return errors != null && errors.length === 0
      ? {
          ok: true,
          value: conversion as TSchemaTranslate<TResolve<T>>
        }
      : {
          ok: false,
          errors: errors.flatMap((error) => {
            if (
              error.type === ValueErrorType.Union &&
              error.schema.errorType?.includes('any of')
            ) {
              return error.errors.flatMap((e, idx) =>
                Array.from(e).map((e) => ({
                  path: [
                    `Union Schema Variant ${idx}`,
                    ...error.path.split('/').slice(1),
                    ...e.path.split('/').slice(1)
                  ],
                  message: e.message
                }))
              );
            } else {
              return [
                {
                  path: error.path.split('/').slice(1),
                  message: error.message
                }
              ];
            }
          })
        };
  }

  /**
   * Convert a schema to an OpenAPI schema object.
   * @param {TIdiomaticSchema | TCatchall} schema - The schema to convert.
   * @returns {SchemaObject} The OpenAPI schema object.
   */
  openapi<T extends TIdiomaticSchema | TCatchall>(schema: T): SchemaObject {
    const schemified: TCatchall = this.schemify(schema);
    let processedSchema: TCatchall;

    if (KindGuard.IsDate(schemified)) {
      processedSchema = Type.String({
        format: 'date-time'
      });
    } else {
      processedSchema = deepCloneWithoutUndefined(schemified);
    }

    const newSchema: SchemaObject = Object.assign({}, processedSchema);

    if (Object.hasOwn(newSchema, 'properties')) {
      if (newSchema.properties) {
        Object.entries({ ...processedSchema.properties }).forEach(
          ([key, value]) => {
            if (KindGuard.IsSchema(value) && newSchema.properties) {
              newSchema.properties[key] = this.openapi(value);
            }
          }
        );
      }
    }
    if (Object.hasOwn(newSchema, 'items')) {
      newSchema.items = this.openapi(newSchema.items as TIdiomaticSchema);
    }
    if (Array.isArray(newSchema.anyOf)) {
      newSchema.anyOf = newSchema.anyOf.map((item) =>
        this.openapi(item as TIdiomaticSchema)
      );
    }
    if (Array.isArray(newSchema.oneOf)) {
      newSchema.oneOf = newSchema.oneOf.map((item) =>
        this.openapi(item as TIdiomaticSchema)
      );
    }

    if ('errorType' in newSchema) {
      delete newSchema['errorType'];
    }

    return newSchema;
  }
}
