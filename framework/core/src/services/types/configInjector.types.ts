import {
  AnySchemaValidator,
  IdiomaticSchema,
  Schema
} from '@forklaunch/validator';
import { ConfigInjector } from '../configInjector';

export const Lifetime = {
  Singleton: 0,
  Transient: 1,
  Scoped: 2
} as const;
export type Lifetime = (typeof Lifetime)[keyof typeof Lifetime];

export type Singleton<Type, Args, Value> =
  | {
      lifetime: typeof Lifetime.Singleton;
      type: Type;
      value: Value;
    }
  | ConstructedSingleton<Type, Args, Value>;

export type ConstructedSingleton<Type, Args, Return> = {
  lifetime: typeof Lifetime.Singleton;
  type: Type;
  factory: (
    args: Args,
    context: Record<string, unknown>,
    resolve: <T extends keyof Args>(
      token: T,
      context?: Record<string, unknown>
    ) => Args[T]
  ) => Return;
};

export type Constructed<Type, Args, Return> = {
  lifetime: typeof Lifetime.Transient | typeof Lifetime.Scoped;
  type: Type;
  factory: (
    args: Args,
    context: Record<string, unknown>,
    resolve: <T extends keyof Args>(
      token: T,
      context?: Record<string, unknown>
    ) => Args[T]
  ) => Return;
};

export type Constructor = new (...args: never[]) => unknown;
export type SchemaConstructor<SV extends AnySchemaValidator> = new (
  ...args: unknown[]
) => IdiomaticSchema<SV>;
export type Function = (...args: never[]) => unknown;
export type FunctionToConstructor = (
  ...args: never[]
) => new (...args: never[]) => unknown;
export type SchemaFunction<SV extends AnySchemaValidator> = (
  args: unknown
) => IdiomaticSchema<SV>;

export type ConfigTypes<SV extends AnySchemaValidator> =
  | Function
  | SchemaFunction<SV>
  | Constructor
  | SchemaConstructor<SV>
  | IdiomaticSchema<SV>;

export type ConfigValidator<SV extends AnySchemaValidator> = Record<
  string,
  ConfigTypes<SV> | Record<string, ConfigTypes<SV>>
>;

type ResolveConfigValue<SV extends AnySchemaValidator, T> =
  T extends SchemaConstructor<SV>
    ? Schema<InstanceType<T>, SV>
    : T extends SchemaFunction<SV>
      ? (...args: Parameters<T>) => Schema<ReturnType<T>, SV>
      : T extends FunctionToConstructor
        ? (...args: Parameters<T>) => InstanceType<ReturnType<T>>
        : T extends Function
          ? (...args: Parameters<T>) => ReturnType<T>
          : T extends Constructor
            ? InstanceType<T>
            : T extends IdiomaticSchema<SV>
              ? Schema<T, SV>
              : T extends Record<string, ConfigTypes<SV>>
                ? {
                    [K in keyof T]: ResolveConfigValue<SV, T[K]>;
                  }
                : Schema<T, SV>;

export type ResolvedConfigValidator<
  SV extends AnySchemaValidator,
  CV extends ConfigValidator<SV>
> = {
  [M in keyof CV]: ResolveConfigValue<SV, CV[M]>;
};

export type ScopedDependencyFactory<
  SV extends AnySchemaValidator,
  CV extends ConfigValidator<SV>,
  M extends keyof CV
> = (scope?: ConfigInjector<SV, CV>) => ResolvedConfigValidator<SV, CV>[M];
