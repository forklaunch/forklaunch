import { ZodLiteral, ZodObject, ZodTypeAny, record, z } from 'zod/v3';
import { generateSchema } from '../../shims/zod-v3-openapi/zod-openapi';
import { UnboxedObjectSchema } from '../../src/shared/types/schema.types';
import { prettyPrintParseErrors } from '../../src/shared/utils/prettyPrintParseErrors';
import {
  array,
  enum_,
  function_,
  isSchema,
  number,
  openapi,
  optional,
  parse,
  promise,
  schemify,
  string,
  union,
  validate
} from '../../src/zod';
import { ZodObjectShape } from '../../src/zod/types/schema.types';
import { ZodSchemaValidator } from '../../src/zod/zodSchemaValidator';
import { compare } from '../utils/compare';

describe('zod schema validator tests', () => {
  let schema: UnboxedObjectSchema<ZodSchemaValidator>;
  let schemified: ZodObject<ZodObjectShape>;
  let expectedSchema: ZodObject<ZodObjectShape>;

  beforeAll(() => {
    schema = {
      hello: {
        world: string
      },
      foo: {
        bar: number
      }
    };
    schemified = schemify(schema);
    expectedSchema = z.object({
      hello: z.object({
        world: z.string()
      }),
      foo: z.object({
        bar: z.number()
      })
    });
  });

  test('schemify', async () => {
    compare(schemified, expectedSchema);

    compare(
      schemified,
      schemify({
        hello: {
          world: string
        },
        foo: {
          bar: number
        }
      })
    );
    compare(
      schemified,
      schemify({
        hello: schemify({
          world: string
        }),
        foo: {
          bar: number
        }
      })
    );

    compare(
      schemified,
      schemify({
        hello: {
          world: string
        },
        foo: schemify({
          bar: number
        })
      })
    );

    compare(
      schemified,
      schemify({
        hello: schemify({
          world: string
        }),
        foo: schemify({
          bar: number
        })
      })
    );
  });

  test('optional', async () => {
    const unboxSchemified = optional(schema);
    const boxSchemified = optional(schemified);

    const schemifiedExpected = expectedSchema.optional();
    compare(unboxSchemified, schemifiedExpected);
    compare(boxSchemified, schemifiedExpected);
  });

  test('array', async () => {
    const unboxSchemified = array(schema);
    const boxSchemified = array(schemified);

    const schemifiedExpected = z.array(expectedSchema);
    compare(unboxSchemified, schemifiedExpected);
    compare(boxSchemified, schemifiedExpected);
  });

  test('union', async () => {
    const unboxSchemified = union([
      schema,
      {
        test: string
      }
    ]);
    const unboxSchemified2 = union([
      schema,
      schemify({
        test: string
      })
    ]);
    const boxSchemified = union([
      schemified,
      schemify({
        test: string
      })
    ]);
    const boxSchemified2 = union([
      schemified,
      {
        test: string
      }
    ]);

    const schemifiedExpected = z.union([
      expectedSchema,
      z.object({
        test: z.string()
      })
    ]);

    compare(unboxSchemified, schemifiedExpected);
    compare(unboxSchemified2, schemifiedExpected);
    compare(boxSchemified, schemifiedExpected);
    compare(boxSchemified2, schemifiedExpected);
  });

  test('literal', async () => {
    const schemified = schemify({
      hello: 'world'
    });
    compare(
      schemified,
      z.object({
        hello: z.literal('world')
      })
    );
  });

  test('enum', async () => {
    const TestSingleEnum = {
      WORLD: 'world'
    } as const;
    const schemified = enum_(TestSingleEnum);

    compare(
      schemified,
      z.union([z.literal('world')] as unknown as [
        ZodLiteral<'world'>,
        ZodTypeAny,
        ...ZodTypeAny[]
      ])
    );

    const TestMultipleEnum = {
      WORLD: 'world',
      HELLO: 'hello'
    } as const;
    const schemifiedMultiple = enum_(TestMultipleEnum);
    compare(
      schemifiedMultiple,
      z.union([z.literal('world'), z.literal('hello')])
    );
  });

  test('function', async () => {
    const schemified = function_([z.string(), z.number()], z.string());
    compare(
      schemified,
      z.function(z.tuple([z.string(), z.number()]), z.string())
    );
  });

  test('record', async () => {
    const schemified = record(z.string(), z.number());
    compare(schemified, z.record(z.string(), z.number()));
  });

  test('promise', async () => {
    const schemified = promise(z.string());
    compare(schemified, z.promise(z.string()));
  });

  test('isSchema', () => {
    expect(isSchema(z.string())).toBe(true);
    expect(isSchema(z.number())).toBe(true);
    expect(isSchema(z.object({ foo: z.string() }))).toBe(true);
    expect(isSchema(schemified)).toBe(true);

    expect(isSchema('not a schema')).toBe(false);
    expect(isSchema(42)).toBe(false);
    expect(isSchema({})).toBe(false);
    expect(isSchema(null)).toBe(false);
    expect(isSchema(undefined)).toBe(false);
  });

  test('validate', async () => {
    validate(schemified, {
      hello: {
        world: 'world'
      },
      foo: {
        bar: 42
      }
    });

    expect(
      validate(schemified, {
        hello: {
          world: 55
        },
        foo: {}
      })
    ).toBe(false);
  });

  test('parse', () => {
    const validParse = parse(schemified, {
      hello: {
        world: 'world'
      },
      foo: {
        bar: 42
      }
    });
    expect(validParse.ok).toBe(true);
    if (validParse.ok) {
      compare(validParse.value, {
        hello: {
          world: 'world'
        },
        foo: {
          bar: 42
        }
      });
    }

    const failedParse = parse(schemified, {
      hello: {
        world: 55
      },
      foo: {}
    });
    expect(failedParse.ok).toBe(false);
    if (!failedParse.ok) {
      expect(prettyPrintParseErrors(failedParse.errors))
        .toBe(`Validation failed with the following errors:
1. Path: hello > world
   Message: Expected string, received number

2. Path: foo > bar
   Message: Expected number, received nan`);
    }
  });

  test('null OpenAPI enum contains JSON null', () => {
    const nullSchema = new ZodSchemaValidator().null;
    expect(generateSchema(nullSchema).enum).toEqual([null]);
    expect(generateSchema(z.null()).enum).toEqual([null]);
  });

  test('openapi', async () => {
    const schemified = schemify(schema);
    const openApi = openapi(schemified);
    expect(openApi).toEqual(generateSchema(schemified));
  });
});
