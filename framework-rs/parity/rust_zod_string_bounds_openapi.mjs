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
const rules = [
  ["min", 0],
  ["min", 1],
  ["min", 3],
  ["max", 0],
  ["max", 2],
  ["max", 4],
  ["length", 0],
  ["length", 2],
  ["length", 3],
];
const sequences = [
  [],
  ...rules.map((x) => [x]),
  ...rules.flatMap((a) => rules.map((b) => [a, b])),
  ...rules.flatMap((a) => rules.flatMap((b) => rules.map((c) => [a, b, c]))),
];
const inputs = [
  null,
  0,
  false,
  {},
  [],
  "",
  "a",
  "ab",
  "abc",
  "abcd",
  "abcde",
  "é",
  "中",
  "e\u0301",
  "\u0000\n",
  "\uffff",
];
const wrappers = [
  (x) => x,
  (x) => z.object({ value: x }),
  (x) => z.array(x),
  (x) => x.optional(),
  (x) => z.union([x, z.literal("escape")]),
];
let comparisons = 0,
  exports = 0,
  controls = 0;
for (const checks of sequences) {
  let leaf = z.string();
  for (const [kind, value] of checks) leaf = leaf[kind](value);
  for (let w = 0; w < wrappers.length; w++)
    for (const profile of ["3.0", "3.1", "pure"]) {
      const schema = wrappers[w](leaf),
        doc =
          profile === "pure"
            ? validator.openapi2020(schema, { pureInput: true })
            : generateSchema(schema, false, profile);
      const validate = new (profile === "pure" ? Ajv2020 : Ajv)({
        strict: false,
      }).compile(doc);
      exports++;
      for (const value of inputs) {
        const input = w === 1 ? { value } : w === 2 ? [value] : value;
        assert.equal(
          Boolean(validate(input)),
          schema.safeParse(input).success,
          JSON.stringify({ checks, profile, w, input, doc }),
        );
        comparisons++;
      }
    }
}
for (const transform of ["trim", "toLowerCase", "toUpperCase"]) {
  const schema = z.string().min(3)[transform]().min(1);
  const doc = validator.openapi(schema);
  assert.equal(doc.minLength, 1);
  controls++; // Explicit unchanged transform-order boundary.
}
for (const value of [-1, 0.5, Infinity, NaN]) {
  const doc = validator.openapi(z.string().min(4).min(value));
  assert.ok(Object.is(doc.minLength, value));
  controls++;
}
for (const version of ["3.0", "3.1"]) {
  const doc = generateSchema(
    z.string().min(4).min(2).openapi({ minLength: 1, description: "override" }),
    false,
    version,
  );
  assert.equal(doc.minLength, 1);
  assert.equal(doc.description, "override");
  controls++;
}
let callbacks = 0;
const callback = () => {
  callbacks++;
  throw Error("unexpected callback");
};
validator.openapi2020(
  z.object({ value: z.string().min(4).min(2).default(callback) }),
  { pureInput: true },
);
assert.equal(callbacks, 0);
controls++;
let unicodeDifferences = 0;
for (const schema of [
  z.string().length(1),
  z.string().length(2),
  z.string().min(2),
  z.string().max(1),
]) {
  const validate = new Ajv({ strict: false }).compile(
    validator.openapi(schema),
  );
  for (const input of ["😀", "a😀", "😀😀"])
    if (Boolean(validate(input)) !== schema.safeParse(input).success)
      unicodeDifferences++;
}
assert.equal(unicodeDifferences, 6);
console.log(
  `PASS ${comparisons} BMP/JSON Zod/AJV string-bound comparisons across ${exports} exports +${controls} transform/invalid-registration/annotation/callback controls; ${unicodeDifferences} explicit remaining UTF16/code-point differences`,
);
