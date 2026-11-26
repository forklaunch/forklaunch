import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
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
const literals = [
  "",
  "ab",
  "a.b",
  "[x]",
  "a*+?",
  "^$",
  "(){}",
  "a|b",
  "\\",
  "-",
  "/",
  "\n",
  "\u0000",
  "😀",
  "é",
  "中",
];
const values = [
  null,
  0,
  false,
  [],
  {},
  "",
  ...literals,
  ...literals.flatMap((x) => ["x" + x, x + "x", "x" + x + "x", x + "\n"]),
];
const leaves = [];
for (const literal of literals) {
  leaves.push(
    z.string().startsWith(literal),
    z.string().endsWith(literal),
    z.string().includes(literal),
    z.string().includes(literal, { position: 0 }),
    z.string().startsWith(literal).endsWith(literal).includes(literal),
  );
}
leaves.push(
  z.string().startsWith("a").startsWith("b"),
  z.string().startsWith("a.").endsWith("z$"),
  z.string().includes("a").includes("b"),
  z.string().regex(/^a/).endsWith("z"),
);
for (const position of [1, -1, Infinity, -Infinity, NaN, 0.5])
  leaves.push(z.string().includes("", { position }));
const wrappers = [
  (x) => x,
  (x) => z.object({ value: x }),
  (x) => z.array(x),
  (x) => z.union([x, z.literal("other")]),
];
let comparisons = 0,
  exports = 0,
  controls = 0;
const native = [];
for (const leaf of leaves)
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
      const cases = values.map((value) => {
        const input = w === 1 ? { value } : w === 2 ? [value] : value;
        const valid = schema.safeParse(input).success;
        assert.equal(
          Boolean(validate(input)),
          valid,
          JSON.stringify({ checks: leaf._def.checks, profile, input, doc }),
        );
        comparisons++;
        return { input, valid };
      });
      if (profile === "3.1") native.push({ schema: doc, cases });
    }
for (const literal of ["\ud800", "\udfff", "a\ud800"])
  for (const kind of ["startsWith", "endsWith", "includes"]) {
    const schema = z.string()[kind](literal);
    assert.throws(
      () => validator.openapi2020(schema, { pureInput: true }),
      /position or surrogate/,
    );
    assert.equal(validator.openapi(schema).allOf, undefined);
    controls++;
  }
for (const position of [1, -1, Infinity, -Infinity, NaN, 0.5]) {
  const schema = z.string().includes("a", { position });
  assert.throws(
    () => validator.openapi2020(schema, { pureInput: true }),
    /position or surrogate/,
  );
  assert.equal(validator.openapi(schema).allOf, undefined);
  controls++;
}
for (const transform of ["trim", "toLowerCase", "toUpperCase"]) {
  assert.equal(
    validator.openapi(z.string()[transform]().startsWith("x")).allOf,
    undefined,
  );
  controls++;
}
const annotated = generateSchema(
  z
    .string()
    .startsWith("x")
    .openapi({
      allOf: [{ maxLength: 3 }],
      pattern: "z",
      description: "annotation",
    }),
  false,
  "3.1",
);
assert.equal(annotated.allOf.length, 2);
assert.equal(annotated.allOf[1].maxLength, 3);
assert.equal(annotated.pattern, "z");
controls++;
let calls = 0;
validator.openapi2020(
  z.object({
    value: z
      .string()
      .endsWith("x")
      .default(() => {
        calls++;
        throw Error("no callback");
      }),
  }),
  { pureInput: true },
);
assert.equal(calls, 0);
controls++;
console.log(
  `PASS ${comparisons} actual Zod/AJV literal-string comparisons across ${exports} exports +${controls} unsupported/transform/annotation/callback controls`,
);
if (process.env.FORKLAUNCH_STRING_LITERAL_NATIVE === "1") {
  const env = {
    ...process.env,
    CARGO_INCREMENTAL: "0",
    CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "2",
  };
  const build = spawnSync(
    "cargo",
    [
      "build",
      "--locked",
      "--manifest-path",
      "framework-rs/Cargo.toml",
      "-p",
      "forklaunch-core",
      "--features",
      "schema-validation",
      "--example",
      "uri_schema_parity",
    ],
    { cwd: root, env, encoding: "utf8" },
  );
  assert.equal(build.status, 0, build.stderr);
  const target = path.resolve(
    process.env.CARGO_TARGET_DIR ?? path.join(root, "framework-rs/target"),
  );
  let count = 0;
  for (const { schema, cases } of native) {
    const result = spawnSync(
      path.join(target, "debug/examples/uri_schema_parity"),
      [],
      {
        cwd: root,
        env,
        encoding: "utf8",
        input:
          [schema, ...cases.map((x) => x.input)]
            .map((x) => JSON.stringify(x))
            .join("\n") + "\n",
      },
    );
    assert.equal(result.status, 0, result.stderr);
    const actual = result.stdout
      .trim()
      .split("\n")
      .map((x) => x === "true");
    assert.deepEqual(
      actual,
      cases.map((x) => x.valid),
      JSON.stringify(schema),
    );
    count += cases.length;
  }
  console.log(
    `PASS ${count} shipped native jsonschema/fancy-regex admission comparisons across ${native.length} emitted schemas`,
  );
}
