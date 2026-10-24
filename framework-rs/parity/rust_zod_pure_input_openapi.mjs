import assert from "node:assert/strict";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { actualValidator } from "./rust_actual_validator_fixture.mjs";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const { validator, load } = actualValidator(root);
const require = createRequire(
  path.join(
    process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES ??
      "/private/tmp/fl-canonical-http-dependencies",
    "package.json",
  ),
);
const { z } = require("zod/v3"),
  Ajv2020 = require("ajv/dist/2020").default;
const { generateSchema } = load(
  path.join(root, "framework/validator/shims/zod-v3-openapi/zod-openapi.ts"),
);
const dialect = "https://json-schema.org/draft/2020-12/schema";
const pure = (schema) => validator.openapi2020(schema, { pureInput: true });
let guards = 0,
  comparisons = 0;
const shapes = [
  (x) => x,
  (x) => z.object({ value: x }),
  (x) => z.array(x),
  (x) => z.tuple([x]),
  (x) => z.record(x),
  (x) => z.record(x, z.string()),
  (x) => x.optional(),
  (x) => x.nullable(),
  (x) => x.readonly(),
  (x) => x.brand(),
  (x) => z.union([z.number(), x]),
  (x) => z.intersection(x, z.unknown()),
];
for (const shape of shapes)
  for (const throws of [false, true]) {
    let calls = 0;
    const leaf = z.string().default(() => {
      calls++;
      if (throws) throw Error("default must not execute");
      return "computed";
    });
    const schema = shape(leaf),
      exported = pure(schema);
    assert.equal(calls, 0);
    assert.equal(JSON.stringify(exported).includes("computed"), false);
    guards++;
  }
for (const kind of [
  "transform",
  "refine",
  "preprocess",
  "catch",
  "lazy",
  "pipeline",
])
  for (const shape of shapes) {
    let calls = 0;
    const callback = () => {
      calls++;
      throw Error("callback must not execute");
    };
    const leaf =
      kind === "transform"
        ? z.string().optional().transform(callback)
        : kind === "refine"
          ? z.string().optional().refine(callback)
          : kind === "preprocess"
            ? z.preprocess(callback, z.string())
            : kind === "catch"
              ? z.string().catch(callback)
              : kind === "lazy"
                ? z.lazy(callback)
                : z.string().pipe(z.string().transform(callback));
    assert.throws(
      () => pure(shape(leaf)),
      /Pure input export does not support/,
    );
    assert.equal(calls, 0);
    guards++;
  }
const children = [
  z.string(),
  z.string().optional(),
  z.string().nullable(),
  z.string().nullish(),
  z.string().default("x"),
  z.string().optional().readonly(),
  z.string().optional().brand(),
  z.string().default("x").readonly(),
  z.union([z.number(), z.string().optional()]),
  z.intersection(z.string().optional(), z.string().optional()),
  z.intersection(z.string().optional(), z.string()),
  z.unknown(),
  z.any(),
];
const inputs = [
  {},
  { value: null },
  { value: "x" },
  { value: 1 },
  { value: false },
  { value: [] },
  { value: {} },
  { value: "x", extra: 1 },
];
for (const child of children)
  for (const policy of ["strip", "strict", "passthrough"]) {
    const schema = z.object({ value: child })[policy]();
    const exported = pure(schema),
      validate = new Ajv2020({ strict: false }).compile(exported);
    for (const input of inputs) {
      assert.equal(
        validate(input),
        validator.parse(schema, input).ok,
        JSON.stringify({ input, exported }),
      );
      comparisons++;
    }
  }
let calls = 0;
const defaulted = z
  .string()
  .default(() => {
    calls++;
    throw Error("no default execution");
  })
  .openapi({ default: "declared", description: "trusted annotation" });
const declared = pure(z.object({ value: defaulted }));
assert.equal(calls, 0);
assert.equal(declared.properties.value.default, "declared");
assert.equal(declared.properties.value.description, "trusted annotation");
assert.equal(declared.required, undefined);
guards++;
const cycle = z.array(z.string());
cycle._def.type = cycle;
assert.throws(() => pure(cycle), /cycle/);
guards++;
let deep = z.string();
for (let i = 0; i < 66; i++) deep = deep.optional();
assert.throws(() => pure(deep), /budget/);
guards++;
const wide = {};
for (let i = 0; i < 10001; i++)
  wide["key" + i] = z.string().default(() => {
    calls++;
    throw Error("wide callback");
  });
assert.throws(() => pure(z.object(wide)), /budget/);
assert.equal(calls, 0);
guards++;
assert.throws(
  () =>
    generateSchema(
      z.string().transform(() => {
        calls++;
        return 1;
      }),
      true,
      "3.1",
      { jsonSchemaDialect: dialect, pureInput: true },
    ),
  /does not support output/,
);
assert.equal(calls, 0);
guards++;
const immutable = z.object({ value: z.string().optional() });
Object.freeze(immutable._def);
const first = pure(immutable);
assert.deepEqual(pure(immutable), first);
guards++;
let legacyCalls = 0;
const legacy = z.object({
  value: z.string().default(() => {
    legacyCalls++;
    return "legacy";
  }),
});
const old = validator.openapi2020(legacy);
assert.equal(old.properties.value.default, "legacy");
assert.equal(legacyCalls, 2);
guards++;
assert.deepEqual(
  validator.openapi2020(z.tuple([z.string()])),
  validator.openapi2020(z.tuple([z.string()]), { pureInput: false }),
);
guards++;
for (const leaf of [z.undefined(), z.void()]) {
  for (const schema of [
    leaf,
    z.object({ value: leaf }),
    z.object({ value: leaf.nullable() }),
    z.array(leaf),
    z.union([leaf, z.string()]),
  ]) {
    const validate = new Ajv2020({ strict: false }).compile(pure(schema));
    for (const input of [
      null,
      false,
      0,
      "",
      [],
      [null],
      {},
      { value: null },
      { value: 0 },
      { value: "" },
    ]) {
      assert.equal(
        validate(input),
        validator.parse(schema, input).ok,
        JSON.stringify({ input, exported: pure(schema) }),
      );
      comparisons++;
    }
  }
}
for (const schema of [
  z.coerce.string(),
  z.coerce.number(),
  z.coerce.boolean(),
  z.coerce.date(),
  z.coerce.bigint(),
  z.map(z.string(), z.string()),
  z.set(z.string()),
  z.promise(z.string()),
  z.function(),
  z.nan(),
  z.symbol(),
  z.bigint(),
  { _def: { typeName: "Unrecognized" } },
]) {
  assert.throws(
    () =>
      generateSchema(schema, false, "3.1", {
        jsonSchemaDialect: dialect,
        pureInput: true,
      }),
    /does not support/,
  );
  guards++;
}
for (const leaf of [
  z.literal(null),
  z.literal(undefined),
  z.literal("x"),
  z.literal(false),
  z.literal(0),
]) {
  for (const schema of [
    leaf,
    z.object({ value: leaf }),
    z.union([leaf, z.literal("other")]),
  ]) {
    const validate = new Ajv2020({ strict: false }).compile(pure(schema));
    for (const input of [
      null,
      false,
      0,
      "x",
      "other",
      [],
      {},
      { value: null },
      { value: false },
      { value: 0 },
      { value: "x" },
    ]) {
      assert.equal(
        validate(input),
        validator.parse(schema, input).ok,
        JSON.stringify({ input, schema: pure(schema) }),
      );
      comparisons++;
    }
  }
}
for (const leaf of [
  z.literal(NaN),
  z.literal(Infinity),
  z.literal(1n),
  z.literal(Symbol("x")),
  z.literal({}),
]) {
  for (const schema of [
    leaf,
    z.object({ value: leaf }),
    z.union([z.literal("x"), leaf]),
  ]) {
    assert.throws(() => pure(schema), /does not support/);
    guards++;
  }
}
for (const transform of ["trim", "toLowerCase", "toUpperCase"]) {
  for (const leaf of [
    z.string()[transform](),
    z.string().min(3)[transform]().min(1),
    z.string().max(1)[transform]().max(1),
  ]) {
    for (const shape of shapes) {
      let defaultCalls = 0;
      const schema = shape(
        leaf.default(() => {
          defaultCalls++;
          throw Error("no default callback");
        }),
      );
      assert.throws(
        () => pure(schema),
        /does not support built-in string transforms/,
      );
      assert.equal(defaultCalls, 0);
      guards++;
    }
  }
  const schema = z.string().min(3)[transform]().min(1);
  assert.equal(validator.openapi2020(schema).minLength, 1);
  assert.equal(
    validator.openapi2020(schema, { pureInput: false }).minLength,
    1,
  );
  guards++;
}
// Actual ordered-transform admission differs from its legacy wire schema.
for (const [schema, input] of [
  [z.string().trim().min(2), " a "],
  [z.string().max(1).toLowerCase().max(1), "İ"],
  [z.string().max(1).toUpperCase().max(1), "ß"],
]) {
  const validate = new Ajv2020({ strict: false }).compile(
    validator.openapi2020(schema),
  );
  assert.equal(validate(input), true);
  assert.equal(schema.safeParse(input).success, false);
  assert.throws(
    () => pure(schema),
    /does not support built-in string transforms/,
  );
  guards++;
}
console.log(
  `PASS ${comparisons} actual Zod/AJV2020 pure input admission comparisons +${guards} callback/annotation/resource/compatibility controls; nested defaults/effects never executed, unsupported callback-dependent schemas rejected, default export unchanged`,
);
