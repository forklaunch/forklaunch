import { actualValidator } from "./rust_actual_validator_fixture.mjs";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const require = createRequire(
  path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES, "package.json"),
);
const { z } = require("zod/v3"),
  Ajv = require("ajv/dist/2020").default,
  addFormats = require("ajv-formats");
const { validator, load } = actualValidator(root, undefined, {
  sameRealm: true,
});
const { registerZodChecks } = load(
  path.join(root, "framework/universal-sdk/src/core/zodChecks.ts"),
);
const values = [
  "",
  "a@b.co",
  "a'b@b-.co",
  "a%b@b.co",
  "a@b.co\n",
  "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
  "00000000-0000-0000-0000-000000000000",
  "urn:uuid:00000000-0000-0000-0000-000000000000",
  "http://example.test/a b",
  "mailto:a@b.co",
  "http://256.1.1.1",
  "http://例え.テスト",
  "水",
  "😀",
  null,
  1,
];
for (const base of [...values].filter((value) => typeof value === "string"))
  for (const suffix of ["\r", "\n", "\r\n", "\u2028", "\0", " ", "."])
    values.push(base + suffix);
const sequences = [];
for (const a of ["email", "uuid", "url"]) {
  sequences.push([a]);
  for (const b of ["email", "uuid", "url"]) {
    sequences.push([a, b]);
    for (const c of ["email", "uuid", "url"]) sequences.push([a, b, c]);
  }
}
const native = process.env.FORKLAUNCH_ZOD_CHECKS_NATIVE === "1",
  target = path.resolve(
    process.env.CARGO_TARGET_DIR ?? path.join(root, "framework-rs/target"),
  );
function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    env: {
      ...process.env,
      CARGO_INCREMENTAL: "0",
      CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "1",
    },
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    ...options,
  });
  assert.equal(result.status, 0, result.stdout + "\n" + result.stderr);
  return result.stdout;
}
if (native)
  run("cargo", [
    "build",
    "--locked",
    "--manifest-path",
    "framework-rs/Cargo.toml",
    "-p",
    "forklaunch-core",
    "--features",
    "zod-uri,zod-checks",
    "--example",
    "uri_schema_parity",
  ]);
assert.equal(process.versions.node, "24.2.0");
assert.equal(process.versions.ada, "3.2.4");
let total = 0,
  nativeCount = 0;
function check(schema, inputs) {
  for (const pureInput of [false, true]) {
    const before = validator.openapi2020(schema, { pureInput }),
      emitted = validator.openapi2020(schema, { pureInput, zodChecks: true });
    assert.deepEqual(
      validator.openapi2020(schema, { pureInput, zodChecks: false }),
      before,
    );
    const engine = registerZodChecks(
      new Ajv({ strict: false, ownProperties: true }),
    );
    addFormats(engine);
    const validate = engine.compile(emitted),
      expected = inputs.map((value) => validator.parse(schema, value).ok);
    inputs.forEach((value, index) => {
      assert.equal(
        !!validate(value),
        expected[index],
        JSON.stringify({ emitted, value }),
      );
      total++;
    });
    if (native) {
      const actual = run(
        path.join(target, "debug/examples/uri_schema_parity"),
        [],
        {
          input:
            [emitted, ...inputs]
              .map((value) => JSON.stringify(value))
              .join("\n") + "\n",
        },
      )
        .trim()
        .split("\n")
        .map(JSON.parse);
      assert.deepEqual(actual, expected);
      nativeCount += actual.length;
    }
  }
}
for (const sequence of sequences)
  for (const factory of ["raw", "email", "uri"]) {
    let schema = factory === "raw" ? z.string() : validator[factory];
    for (const method of sequence) schema = schema[method]();
    check(schema, values);
  }
for (const schema of [
  z.string().email().min(2).max(100),
  z.string().uuid().startsWith("0"),
  validator.uri.min(3),
  z.string().url().regex(/^http/),
])
  check(schema, values);
for (const schema of [
  validator.email.regex(/^a/),
  validator.email.regex(/^a/).email(),
  z.string().regex(/^a/).email(),
  z.string().email().regex(/^a/),
]) {
  check(schema, values);
  assert.equal(
    validator.openapi2020(schema, { zodChecks: true }).pattern,
    "^a",
  );
}
for (const base of [z.string().email(), z.string().uuid(), z.string().url()]) {
  check(z.object({ value: base.optional() }), [
    {},
    ...values.map((value) => ({ value })),
  ]);
  check(
    z.array(base),
    values.map((value) => [value]),
  );
  check(z.union([base, z.literal("alternate")]), [...values, "alternate"]);
}
for (const annotation of [
  { format: "email" },
  { format: undefined },
  { format: null },
]) {
  const schema = z.string().uuid().url().openapi(annotation),
    out = validator.openapi2020(schema, { zodChecks: true });
  assert.equal(out.format, annotation.format);
  assert.equal(out["x-forklaunch-zod-checks"], undefined);
}
for (const annotation of [
  { pattern: "^custom$" },
  { pattern: undefined },
  { pattern: null },
]) {
  const out = validator.openapi2020(validator.email.openapi(annotation), {
    zodChecks: true,
  });
  assert.equal(out.pattern, annotation.pattern);
  assert.equal(out["x-forklaunch-zod-checks"], undefined);
}
// A raw email's explicit pattern remains conjunctive; only its format replaces its format check.
const raw = validator.openapi2020(
  z.string().email().openapi({ pattern: "^a" }),
  { zodChecks: true },
);
assert.equal(raw.pattern, "^a");
assert.deepEqual(raw["x-forklaunch-zod-checks"].formats, ["email"]);
assert.deepEqual(
  validator.openapi2020(z.string().email().cuid(), { zodChecks: true })["x-forklaunch-zod-checks"].formats,
  ["email", "cuid"],
);
assert.throws(
  () => validator.openapi2020(z.string().trim().email(), { zodChecks: true }),
  /transformations/,
);
assert.equal(
  validator.openapi2020(
    z.string().trim().email().openapi({ format: "email" }),
    { zodChecks: true },
  )["x-forklaunch-zod-checks"],
  undefined,
);
let callbacks = 0;
validator.openapi2020(
  z.object({
    value: z
      .string()
      .email()
      .default(() => {
        callbacks++;
        throw new Error("unused");
      }),
  }),
  { pureInput: true, zodChecks: true },
);
assert.equal(callbacks, 0);
for (const formats of [
  null,
  {},
  ["url"],
  ["EMAIL"],
  [null],
  Array(10001).fill("email"),
])
  assert.throws(
    () =>
      registerZodChecks(new Ajv({ strict: false })).compile({
        "x-forklaunch-zod-checks": { version: 1, formats },
      }),
    /Zod/,
  );
assert.throws(
  () =>
    registerZodChecks(new Ajv({ strict: false })).compile({
      "x-forklaunch-zod-checks": {
        version: 1,
        formats: Array(5001).fill("email"),
        multipleOf: Array(5000).fill(1),
      },
    }),
  /10000/,
);
console.log(
  `PASS ${total} actual Zod/AJV ordered format conjunctions and ${nativeCount} native checks; raw/factory, overrides, nested/pure/callback0, config limits`,
);
