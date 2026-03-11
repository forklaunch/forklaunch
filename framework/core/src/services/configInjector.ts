import { extractArgumentNames, isNever } from '@forklaunch/common';
import {
  AnySchemaValidator,
  IdiomaticSchema,
  ParseResult,
  prettyPrintParseErrors,
  SchemaValidator
} from '@forklaunch/validator';
import { isConstructed } from './guards/isConstructed';
import { isConstructedSingleton } from './guards/isConstructedSingleton';
import { isConstructor } from './guards/isConstructor';
import {
  ConfigValidator,
  Constructed,
  ConstructedSingleton,
  Lifetime,
  ResolvedConfigValidator,
  SchemaConstructor,
  SchemaFunction,
  Singleton
} from './types/configInjector.types';

export function createConfigInjector<
  SV extends AnySchemaValidator,
  CV extends ConfigValidator<SV>
>(
  schemaValidator: SV,
  dependenciesDefinition: {
    [K in keyof CV]:
      | Singleton<
          CV[K],
          Omit<ResolvedConfigValidator<SV, CV>, K>,
          ResolvedConfigValidator<SV, CV>[K]
        >
      | Constructed<
          CV[K],
          Omit<ResolvedConfigValidator<SV, CV>, K>,
          ResolvedConfigValidator<SV, CV>[K]
        >;
  }
) {
  return new ConfigInjector<SV, CV>(
    schemaValidator,
    dependenciesDefinition
  ).load();
}

export class ConfigInjector<
  SV extends AnySchemaValidator,
  CV extends ConfigValidator<SV>
> {
  instances: {
    [K in keyof CV]?: ResolvedConfigValidator<SV, CV>[K];
  } = {};

  readonly configShapes: CV;

  load(inheritedScopeInstances?: {
    [K in keyof CV]?: ResolvedConfigValidator<SV, CV>[K];
  }): this {
    for (const token in inheritedScopeInstances) {
      this.instances[token] = inheritedScopeInstances[token];
    }

    for (const token in this.dependenciesDefinition) {
      const definition = this.dependenciesDefinition[token];
      if (
        definition.lifetime === Lifetime.Singleton &&
        !this.instances[token]
      ) {
        if (
          isConstructedSingleton<
            CV[typeof token],
            Omit<ResolvedConfigValidator<SV, CV>, typeof token>,
            ResolvedConfigValidator<SV, CV>[typeof token]
          >(definition)
        ) {
          this.instances[token] = this.resolveInstance<typeof token>(
            token,
            definition
          );
        } else {
          this.instances[token] = definition.value;
        }
      }
    }
    return this;
  }

  private resolveInstance<T extends keyof CV>(
    token: T,
    definition:
      | ConstructedSingleton<
          CV[T],
          Omit<ResolvedConfigValidator<SV, CV>, T>,
          ResolvedConfigValidator<SV, CV>[T]
        >
      | Constructed<
          CV[T],
          Omit<ResolvedConfigValidator<SV, CV>, T>,
          ResolvedConfigValidator<SV, CV>[T]
        >,
    context?: Record<string, unknown>,
    resolutionPath: (keyof CV)[] = []
  ): ResolvedConfigValidator<SV, CV>[T] {
    if (process.env.FORKLAUNCH_MODE === 'openapi') {
      const noopProxy: Record<string, unknown> = new Proxy(
        function () {} as unknown as Record<string, unknown>,
        {
          get(_target, prop) {
            if (prop === Symbol.toPrimitive) return () => '';
            if (prop === 'toString' || prop === 'valueOf') return () => '';
            if (prop === 'then')
              return (resolve: (value: unknown) => void) => resolve(noopProxy);
            if (prop === Symbol.iterator) return function* () {};
            return noopProxy;
          },
          set() {
            return true;
          },
          has() {
            return true;
          },
          ownKeys() {
            return [];
          },
          getOwnPropertyDescriptor() {
            return { configurable: true, enumerable: true };
          },
          apply() {
            return noopProxy;
          },
          construct() {
            return noopProxy as object;
          }
        }
      );
      return noopProxy as ResolvedConfigValidator<SV, CV>[T];
    }

    const rawInjectorArgument = extractArgumentNames(definition.factory)[0];
    // short circuit as no args
    if (!rawInjectorArgument || rawInjectorArgument === '_args') {
      return definition.factory(
        {} as Omit<ResolvedConfigValidator<SV, CV>, T>,
        this.resolve.bind(this),
        context ?? ({} as Record<string, unknown>)
      );
    }

    // Normalize whitespace to handle both single-line and multiline formatting
    const injectorArgument = rawInjectorArgument.replace(/\s+/g, '');

    if (!injectorArgument.startsWith('{') || !injectorArgument.endsWith('}')) {
      throw new Error(
        `Invalid injector argument for ${String(
          token
        )}: ${injectorArgument}. Please use object destructuring syntax: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Destructuring_assignment.`
      );
    }
    const resolvedArguments = Object.fromEntries(
      injectorArgument
        .slice(1, -1)
        .split(',')
        .filter((arg) => arg.length > 0)
        .map((arg) => arg.split(':')[0].trim())
        .map((arg) => {
          const newResolutionPath = [...resolutionPath, token];
          if (resolutionPath.includes(arg)) {
            throw new Error(
              `Circular dependency detected: ${newResolutionPath.join(
                ' -> '
              )} -> ${arg}`
            );
          }
          const resolvedArg = this.resolve(arg, context, newResolutionPath);
          return [arg, resolvedArg];
        })
    ) as unknown as Omit<ResolvedConfigValidator<SV, CV>, T>;
    return definition.factory(
      resolvedArguments,
      this.resolve.bind(this),
      context ?? ({} as Record<string, unknown>)
    );
  }

  constructor(
    private schemaValidator: SV,
    private dependenciesDefinition: {
      [K in keyof CV]: (
        | Singleton<
            CV[K],
            Omit<ResolvedConfigValidator<SV, CV>, K>,
            ResolvedConfigValidator<SV, CV>[K]
          >
        | Constructed<
            CV[K],
            Omit<ResolvedConfigValidator<SV, CV>, K>,
            ResolvedConfigValidator<SV, CV>[K]
          >
      ) & {
        type: CV[K];
      };
    }
  ) {
    this.configShapes = Object.entries(this.dependenciesDefinition).reduce(
      (acc, [key, { type }]) => ({
        ...acc,
        [key]: type
      }),
      {} as Record<keyof CV, CV[keyof CV]>
    ) as CV;
  }

  safeValidateConfigSingletons(): ParseResult<ValidConfigInjector<SV, CV>> {
    const validNonSchemaSingletons = Object.entries(this.configShapes).reduce<
      ParseResult<ResolvedConfigValidator<SV, CV>>
    >(
      (acc, [key, value]) => {
        if (
          this.dependenciesDefinition[key].lifetime === Lifetime.Singleton &&
          !(this.schemaValidator as SchemaValidator).isSchema<
            SchemaFunction<SV> | SchemaConstructor<SV> | IdiomaticSchema<SV>
          >(value) &&
          isConstructor(value)
        ) {
          if (!(this.instances[key] instanceof value)) {
            const expected = value.name;
            const receivedValue: unknown = this.instances[key];
            const received = isConstructed(receivedValue)
              ? receivedValue.constructor.name
              : typeof receivedValue;

            if (acc.ok) {
              acc = {
                ok: false,
                errors: []
              };
            }
            acc.errors?.push({
              message: `Expected ${expected}, received ${received}`,
              path: [key]
            });
          } else {
            if (acc.ok) {
              acc = {
                ok: true,
                value: {
                  ...acc.value,
                  [key]: this.instances[key]
                }
              };
            }
          }
          return acc;
        }
        return acc;
      },
      {
        ok: true,
        value: {} as ResolvedConfigValidator<SV, CV>
      }
    );

    const singletons = Object.fromEntries(
      Object.entries(this.configShapes).filter(
        ([key, value]) =>
          this.dependenciesDefinition[key].lifetime === Lifetime.Singleton &&
          (this.schemaValidator as SchemaValidator).isSchema(value)
      )
    );
    const schemaSingletonParseResult = (
      this.schemaValidator as SchemaValidator
    ).parse(
      (this.schemaValidator as SchemaValidator).schemify(singletons),
      Object.fromEntries(
        Object.keys(singletons).map((key) => {
          const dependency = this.dependenciesDefinition[key];
          return [
            key,
            dependency.lifetime === Lifetime.Singleton
              ? this.instances[key]
              : undefined
          ];
        })
      )
    );

    const configKeys = Object.keys(this.configShapes);

    return validNonSchemaSingletons.ok && schemaSingletonParseResult.ok
      ? {
          ok: true as const,
          value: new ValidConfigInjector<SV, CV>(
            this.schemaValidator,
            this.dependenciesDefinition
          ).load({ ...this.instances })
        }
      : {
          ok: false as const,
          errors: [
            ...(!validNonSchemaSingletons.ok && validNonSchemaSingletons.errors
              ? validNonSchemaSingletons.errors
              : []),
            ...(!schemaSingletonParseResult.ok &&
            schemaSingletonParseResult.errors
              ? schemaSingletonParseResult.errors
              : [])
          ].sort(
            (a, b) =>
              configKeys.indexOf(a.path[0]) - configKeys.indexOf(b.path[0])
          )
        };
  }

  validateConfigSingletons(configName: string): ValidConfigInjector<SV, CV> {
    if (process.env.FORKLAUNCH_MODE === 'openapi') {
      return this.createScope() as ValidConfigInjector<SV, CV>;
    }

    const safeValidateResult = this.safeValidateConfigSingletons();

    if (safeValidateResult.ok) {
      return safeValidateResult.value;
    }

    throw new Error(
      prettyPrintParseErrors(safeValidateResult.errors, configName)
    );
  }

  resolve<T extends keyof CV>(
    token: T,
    context?: Record<string, unknown>,
    resolutionPath: (keyof CV)[] = []
  ): ResolvedConfigValidator<SV, CV>[T] {
    const instance = this.instances[token];
    if (!instance) {
      const definition = this.dependenciesDefinition[token];

      if (!definition) {
        throw new Error(`Unable to resolve dependency ${String(token)}`);
      }

      switch (definition.lifetime) {
        case Lifetime.Singleton: {
          if (
            isConstructedSingleton<
              CV[T],
              Omit<ResolvedConfigValidator<SV, CV>, T>,
              ResolvedConfigValidator<SV, CV>[T]
            >(definition) &&
            !this.instances[token]
          ) {
            this.instances[token] = this.resolveInstance<T>(
              token,
              definition,
              context,
              resolutionPath
            );
          }
          return this.instances[token] as ResolvedConfigValidator<SV, CV>[T];
        }
        case Lifetime.Scoped: {
          if (
            !isConstructed<
              CV[T],
              Omit<ResolvedConfigValidator<SV, CV>, T>,
              ResolvedConfigValidator<SV, CV>[T]
            >(definition)
          ) {
            throw new Error(
              `Invalid dependency definition for ${String(token)}`
            );
          }

          const scopedInstance = this.resolveInstance<T>(
            token,
            definition,
            context,
            resolutionPath
          );
          this.instances[token] = scopedInstance;
          return scopedInstance;
        }
        case Lifetime.Transient: {
          if (
            !isConstructed<
              CV[T],
              Omit<ResolvedConfigValidator<SV, CV>, T>,
              ResolvedConfigValidator<SV, CV>[T]
            >(definition)
          ) {
            throw new Error(
              `Invalid dependency definition for ${String(token)}`
            );
          }

          return this.resolveInstance<T>(
            token,
            definition,
            context,
            resolutionPath
          );
        }
        default: {
          isNever(definition);
          throw new Error(
            `Unable to resolve lifetime for dependency ${String(
              token
            )}, ${resolutionPath}`
          );
        }
      }
    } else {
      return instance;
    }
  }

  scopedResolver<T extends keyof CV>(
    token: T,
    context?: Record<string, unknown>,
    resolutionPath: (keyof CV)[] = []
  ): (scope?: ConfigInjector<SV, CV>) => ResolvedConfigValidator<SV, CV>[T] {
    return (scope) =>
      (scope ?? this.createScope()).resolve<T>(token, context, resolutionPath);
  }

  createScope(): ConfigInjector<SV, CV> {
    const singletons: Record<string, unknown> = {};
    for (const dependency in this.dependenciesDefinition) {
      if (
        this.dependenciesDefinition[dependency].lifetime === Lifetime.Singleton
      ) {
        singletons[dependency] = this.instances[dependency];
      }
    }
    return new ConfigInjector<SV, CV>(
      this.schemaValidator,
      this.dependenciesDefinition
    ).load(singletons as ResolvedConfigValidator<SV, CV>);
  }

  dispose(): void {
    this.instances = {};
    this.load();
  }

  chain<ChainedCV extends ConfigValidator<SV>>(dependenciesDefinition: {
    [K in keyof ChainedCV]: {
      type: ChainedCV[K];
    } & (
      | Singleton<
          ChainedCV[K],
          Omit<ResolvedConfigValidator<SV, CV & ChainedCV>, K>,
          ResolvedConfigValidator<SV, ChainedCV>[K]
        >
      | Constructed<
          ChainedCV[K],
          Omit<ResolvedConfigValidator<SV, CV & ChainedCV>, K>,
          ResolvedConfigValidator<SV, ChainedCV>[K]
        >
    );
  }): ConfigInjector<SV, CV & ChainedCV> {
    return new ConfigInjector<SV, CV>(this.schemaValidator, {
      ...this.dependenciesDefinition,
      ...dependenciesDefinition
    }).load({ ...this.instances }) as unknown as ConfigInjector<
      SV,
      CV & ChainedCV
    >;
  }

  tokens(): {
    [K in keyof CV]: K;
  } {
    return Object.fromEntries(
      Object.keys(this.dependenciesDefinition).map((key) => [key, key])
    ) as {
      [K in keyof CV]: K;
    };
  }
}

export class ValidConfigInjector<
  SV extends AnySchemaValidator,
  CV extends ConfigValidator<SV>
> extends ConfigInjector<SV, CV> {
  validConfigInjector!: void;
}
