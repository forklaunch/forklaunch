/* eslint-disable @typescript-eslint/no-unused-vars */
import {
  array,
  number,
  optional,
  SchemaValidator,
  string
} from '@forklaunch/validator/typebox';
import {
  ConfigInjector,
  createConfigInjector
} from '../src/services/configInjector';
import { Lifetime } from '../src/services/types/configInjector.types';

class X {
  constructor(
    public a: string,
    public b: () => number
  ) {}

  dummy() {
    return `${this.a}${this.b()}`;
  }
}

describe('serviceFactory', () => {
  const staticX = new X('a', () => 5);
  const staticI = {
    j: 'a',
    k: 'b' as unknown as number
  };
  const configInjector = createConfigInjector(SchemaValidator(), {
    a: {
      type: string,
      lifetime: Lifetime.Singleton,
      value: 'a'
    },
    b: {
      type: number,
      lifetime: Lifetime.Transient,
      factory: ({ a }) => 5
    },
    c: {
      type: X,
      lifetime: Lifetime.Scoped,
      factory: ({ a }, resolve, context) => {
        const b = () => resolve('b') * (context.num as number);
        return new X(a, b);
      }
    },
    d: {
      type: number,
      lifetime: Lifetime.Transient,
      factory: ({ a, e }) => e
    },
    e: {
      type: number,
      lifetime: Lifetime.Transient,
      factory: ({ f }) => f
    },
    f: {
      type: number,
      lifetime: Lifetime.Transient,
      factory: ({ d }) => d
    },
    g: {
      type: number,
      lifetime: Lifetime.Transient,
      factory: () => 6
    },
    h: {
      type: X,
      lifetime: Lifetime.Singleton,
      value: staticX
    },
    i: {
      type: {
        j: string,
        k: number,
        l: X
      },
      lifetime: Lifetime.Singleton,
      value: { ...staticI, l: staticX }
    },
    j: {
      type: string,
      lifetime: Lifetime.Singleton,
      factory: ({ a, g }) => a
    }
  });

  const configInjector2 = configInjector.chain({
    k: {
      type: number,
      lifetime: Lifetime.Transient,
      factory: ({ i }) => i.k
    },
    l: {
      type: (a: string) => ({
        SchemaDefinition: {
          j: '',
          k: 4
        }
      }),
      lifetime: Lifetime.Scoped,
      factory:
        ({ i }) =>
        (a: string) => ({
          SchemaDefinition: {
            j: '',
            k: 4
          },
          ...i,
          m: i.l,
          l: 'l'
        })
    },
    m: {
      type: optional(number),
      lifetime: Lifetime.Singleton,
      value: 4
    },
    o: {
      type: array(number),
      lifetime: Lifetime.Singleton,
      value: [1, 2, 3]
    }
  });

  test('load', () => {
    expect(configInjector.instances).toEqual({
      a: 'a',
      h: staticX,
      i: {
        ...staticI,
        l: staticX
      },
      j: 'a'
    });
  });

  test('basicResolve', () => {
    expect(configInjector.resolve('a')).toBe('a');
    expect(configInjector.resolve('b')).toBe(5);
    expect(
      configInjector
        .resolve('c', {
          num: 4
        })
        .dummy()
    ).toBe('a20');
    expect(Object.keys(configInjector.instances)).toEqual([
      'a',
      'h',
      'i',
      'j',
      'c'
    ]);
  });

  test('circular dependency', () => {
    expect(() =>
      configInjector.resolve('d')
    ).toThrowErrorMatchingInlineSnapshot(
      '[Error: Circular dependency detected: d -> e -> f -> d]'
    );
    expect(() =>
      configInjector.resolve('e')
    ).toThrowErrorMatchingInlineSnapshot(
      '[Error: Circular dependency detected: e -> f -> d -> e]'
    );
    expect(() => {
      configInjector.resolve('f');
    }).toThrowErrorMatchingInlineSnapshot(
      '[Error: Circular dependency detected: f -> d -> e -> f]'
    );
  });

  test('validateConfigSingletons', () => {
    expect(configInjector.safeValidateConfigSingletons().ok).toBe(true);
    const newConfigInjector = createConfigInjector(SchemaValidator(), {
      a: {
        type: string,
        lifetime: Lifetime.Singleton,
        value: 4 as unknown as string
      },
      b: {
        type: X,
        lifetime: Lifetime.Singleton,
        value: 'a' as unknown as X
      },
      c: {
        type: X,
        lifetime: Lifetime.Singleton,
        value: new ConfigInjector(SchemaValidator(), {}) as unknown as X
      }
    });
    const badResult = newConfigInjector.safeValidateConfigSingletons();
    expect(badResult.ok).toBe(false);
    expect(!badResult.ok && badResult.errors).toEqual([
      { path: ['a'], message: 'Expected string' },
      { path: ['b'], message: 'Expected X, received string' },
      { path: ['c'], message: 'Expected X, received ConfigInjector' }
    ]);
  });

  test('createScope', () => {
    const scope = configInjector.createScope();
    expect(scope).toBeInstanceOf(ConfigInjector);
    expect(scope.instances).not.toEqual(configInjector.instances);
  });

  test('scoped resolver', () => {
    const scope = configInjector.createScope();
    scope.resolve('c', {
      num: 5
    });
    expect(configInjector.resolve('c').dummy()).toEqual('a20');
    expect(configInjector.scopedResolver('c')(scope).dummy()).toEqual('a25');
  });

  test('dispose', () => {
    configInjector.dispose();
    expect(configInjector.instances).toEqual({
      a: 'a',
      h: staticX,
      i: {
        ...staticI,
        l: staticX
      },
      j: 'a'
    });
  });

  test('resolve with multiline factory destructuring', () => {
    const multilineInjector = createConfigInjector(SchemaValidator(), {
      x: {
        type: string,
        lifetime: Lifetime.Singleton,
        value: 'hello'
      },
      y: {
        type: number,
        lifetime: Lifetime.Singleton,
        value: 42
      },
      z: {
        type: string,
        lifetime: Lifetime.Transient,
        factory: ({ x, y }: { x: string; y: number }) => `${x}-${y}`
      }
    });
    expect(multilineInjector.resolve('z')).toBe('hello-42');
  });

  test('resolve with multiline factory destructuring and resolve/context', () => {
    const multilineInjector = createConfigInjector(SchemaValidator(), {
      x: {
        type: string,
        lifetime: Lifetime.Singleton,
        value: 'hello'
      },
      y: {
        type: number,
        lifetime: Lifetime.Transient,
        factory: ({ x }, resolve, context) =>
          x.length + (context.extra as number)
      }
    });
    expect(multilineInjector.resolve('y', { extra: 10 })).toBe(15);
  });

  test('chained config injector', () => {
    expect({
      ...configInjector.instances,
      m: 4,
      o: [1, 2, 3]
    }).toEqual(configInjector2.instances);
    expect(configInjector2.resolve('a')).toBe('a');
    expect(configInjector2.resolve('k')).toBe('b');
    expect(configInjector2.resolve('l')('a')).toEqual(
      expect.objectContaining({
        SchemaDefinition: { j: '', k: 4 },
        j: 'a',
        k: 'b',
        l: 'l'
      })
    );
    expect({
      ...configInjector.instances,
      l: configInjector2.instances.l,
      m: 4,
      o: [1, 2, 3]
    }).toEqual(configInjector2.instances);
  });
});
