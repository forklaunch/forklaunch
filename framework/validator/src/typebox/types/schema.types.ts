import {
  StaticDecode,
  TArray,
  TKind,
  TLiteral,
  TNever,
  TObject,
  TProperties,
  TSchema,
  TUnknown
} from '@sinclair/typebox';
import { TypeCheck } from '@sinclair/typebox/compiler';
import {
  IdiomaticSchema,
  Increment,
  KeyTypes,
  LiteralSchema
} from '../../shared/types/schema.types';
import { TypeboxSchemaValidator } from '../typeboxSchemaValidator';

/**
 * Represents a catch-all schema type.
 */
export type TCatchall = TSchema;

/**
 * Represents an outer array schema type. If the type T is an object shape, it will return an array schema of T. Otherwise, it returns TNever.
 *
 * @template T - The type to check and possibly convert to an array schema.
 */
export type TOuterArray<T> =
  T extends TObject<TObjectShape> ? TArray<T> : TNever;

/**
 * Represents the shape of an object schema.
 */
export type TObjectShape = TProperties;

/**
 * Represents an object schema type. If the type T is an object shape, it will return the original TObject type of T. Otherwise, it returns TNever.
 *
 * @template T - The type to check and possibly convert to an object schema.
 */
export type SafeTObject<T> = T extends TObjectShape ? TObject<T> : TNever;

/**
 * Translates a schema type T to its static type if T extends TCatchall. Otherwise, it returns TNever.
 *
 * @template T - The schema type to translate.
 */
export type TSchemaTranslate<T> = T extends TSchema ? StaticDecode<T> : TNever;

/**
 * Represents an unboxed object schema where each key can have an idiomatic schema.
 */
export type UnboxedTObjectSchema = {
  [key: KeyTypes]: TIdiomaticSchema;
};

/**
 * Represents an idiomatic schema which can be an unboxed object schema or a literal schema.
 */
export type TIdiomaticSchema = IdiomaticSchema<TypeboxSchemaValidator>;

/**
 * Represents a container for a union of idiomatic schemas.
 */
export type TUnionTupleContainer = [...TIdiomaticSchema[]];

/**
 * Resolves a union container to a tuple of resolved idiomatic schemas.
 *
 * @template T - The union container to resolve.
 */
export type UnionTupleTResolve<
  T extends TUnionTupleContainer,
  Acc extends TIdiomaticSchema[] = []
> = T extends [
  infer Head extends TIdiomaticSchema,
  ...infer Tail extends TUnionTupleContainer
]
  ? UnionTupleTResolve<Tail, [...Acc, TResolve<Head>]>
  : T extends []
    ? Acc
    : TNever[];

/**
 * Resolves a schema type T to its resolved type. The depth is limited to 12 to prevent infinite recursion, due to StaticDecode limitations.
 *
 * @template T - The schema type to resolve.
 * @template Depth - The current depth of the resolution.
 */
export type TResolve<T, Depth extends number = 0> = Depth extends 12
  ? TUnknown
  : T extends TSchema
    ? T
    : T extends TKind
      ? T
      : T extends LiteralSchema
        ? TLiteral<T>
        : T extends UnboxedTObjectSchema
          ? SafeTObject<{
              [K in keyof T]: TResolve<T[K], Increment<Depth>>;
            }> extends infer R
            ? R
            : TNever
          : T extends TypeCheck<infer Type>
            ? TResolve<Type, Increment<Depth>>
            : TNever;
