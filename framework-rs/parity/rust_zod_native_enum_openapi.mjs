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
  Ajv = require("ajv"),
  Ajv2020 = require("ajv/dist/2020").default;
const { generateSchema } = load(
  path.join(root, "framework/validator/shims/zod-v3-openapi/zod-openapi.ts"),
);
const dialect = "https://json-schema.org/draft/2020-12/schema";
const enums = [
  {},
  { A: "a" },
  { A: 0 },
  { A: -0 },
  { A: -1, B: 0.5 },
  { 0: "A", 1: "B", A: 0, B: 1 },
  { 0: "A", A: 0, B: "b" },
  { A: "a", B: "b" },
  { A: "a", B: "a" },
  { A: 0, B: 0 },
  { A: "0", B: 0 },
  { "01": "x", X: 1 },
  JSON.parse('{"__proto__":"p","constructor":"c","prototype":"q","A":"a"}'),
  { A: "x\u0000\n😀", B: 9007199254740992 },
  { A: "B", B: 1 },
];
const inputs = [
  null,
  false,
  true,
  0,
  -0,
  1,
  -1,
  0.5,
  2,
  9007199254740992,
  "0",
  "A",
  "B",
  "a",
  "b",
  "x",
  "p",
  "c",
  "q",
  "x\u0000\n😀",
  [],
  {},
];
const shapes = [
  (x) => x,
  (x) => z.object({ value: x }),
  (x) => z.array(x),
  (x) => z.tuple([x]),
  (x) => z.union([x, z.literal("other")]),
  (x) => x.optional(),
];
let comparisons = 0,
  exports = 0,
  controls = 0;
for (const values of enums)
  for (const shape of shapes) {
    const schema = shape(z.nativeEnum(values));
    for (const profile of ["3.0", "3.1", "pure"]) {
      if (profile !== "pure" && schema._def.typeName === "ZodTuple") continue; // Explicit tuple projection is opt-in only.
      const doc =
        profile === "pure"
          ? validator.openapi2020(schema, { pureInput: true })
          : generateSchema(schema, false, profile);
      const validate = new (profile === "3.0" ? Ajv : Ajv2020)({
        strict: false,
      }).compile(doc);
      exports++;
      for (const input of [
        ...inputs,
        ...inputs.map((value) => ({ value })),
        ...inputs.map((value) => [value]),
        "other",
      ]) {
        assert.equal(
          Boolean(validate(input)),
          schema.safeParse(input).success,
          JSON.stringify({ values, profile, input, doc }),
        );
        comparisons++;
      }
    }
  }
for (const values of [
  { A: Infinity },
  { A: -Infinity },
  { A: NaN },
  { A: 1n },
  { A: Symbol("x") },
  { A: true },
]) {
  for (const shape of shapes) {
    assert.throws(
      () =>
        validator.openapi2020(shape(z.nativeEnum(values)), { pureInput: true }),
      /does not support/,
    );
    controls++;
  }
}
const enumObject = { 0: "A", A: 0 };
assert.equal(validator.parse(validator.enum_(enumObject), "A").ok, true);
assert.equal(z.nativeEnum(enumObject).safeParse("A").success, false);
controls++;
for (const version of ["3.0", "3.1"]) {
  const doc = generateSchema(
    z
      .nativeEnum({ A: 0, B: "b" })
      .openapi({ type: "string", enum: ["override"], description: "explicit" }),
    false,
    version,
  );
  assert.deepEqual(doc.enum, [0, "b", "override"]);
  assert.deepEqual(doc.type, "string");
  assert.equal(doc.description, "explicit");
  controls++;
}
let calls = 0;
const schema = z.object({
  value: z.nativeEnum({ 0: "A", A: 0 }).default(() => {
    calls++;
    throw Error("no call");
  }),
});
const doc = validator.openapi2020(schema, { pureInput: true });
assert.equal(calls, 0);
assert.equal(doc.required, undefined);
assert.deepEqual(Array.from(doc.properties.value.enum), [0]);
controls++;
assert.deepEqual(Array.from(validator.openapi2020(z.nativeEnum({ A: Infinity })).enum), [
  Infinity,
]);
controls++;
console.log(
  `PASS ${comparisons} actual Zod/AJV native-enum comparisons across ${exports} OAS3.0/3.1/pure exports +${controls} override/nonfinite/factory/callback controls; numeric reverse maps, mixed values, duplicates, empty and private keys`,
);
