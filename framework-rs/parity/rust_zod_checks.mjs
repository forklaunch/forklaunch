import { actualValidator } from "./rust_actual_validator_fixture.mjs";
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
const require = createRequire(
  path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES, "package.json"),
);
const { z } = require("zod/v3"),
  Ajv = require("ajv"),
  Ajv2020 = require("ajv/dist/2020").default,
  formats = require("ajv-formats");
const { validator, load } = actualValidator(root, undefined, {
  sameRealm: true,
});
const { registerZodChecks } = load(
  path.join(root, "framework/universal-sdk/src/core/zodChecks.ts"),
);
assert.equal(
  readFileSync(
    path.join(root, "framework/universal-sdk/src/core/zodChecks.ts"),
    "utf8",
  ),
  readFileSync(
    path.join(
      root,
      "framework-rs/crates/core/src/typescript/zod-checks.ts",
    ),
    "utf8",
  ),
  "self-contained socket capability must match UniversalSdk",
);
const native = process.env.FORKLAUNCH_ZOD_CHECKS_NATIVE === "1";
const target = path.resolve(
  process.env.CARGO_TARGET_DIR ?? path.join(root, "framework-rs/target"),
);
function run(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, {
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
const strings = [
  "",
  "a",
  "A",
  "aa",
  "z",
  "az",
  "az\n",
  "😀",
  "😀a",
  "水",
  "a\u0301",
  "a\u0000",
  "x@y.com",
  "x@y-.com",
  "a%z@x.test",
  "123e4567-e89b-12d3-a456-426614174000",
  "urn:uuid:123e4567-e89b-12d3-a456-426614174000",
  "http://example.test/a b",
  "http://例え.テスト",
  "http://256.1.1.1",
  null,
  0,
  {},
  [],
];
const numbers = [
  null,
  "3",
  -9,
  -6,
  -3,
  -2,
  -1,
  -0.3,
  -0.2,
  -0.1,
  -0,
  0,
  0.1,
  0.2,
  0.3,
  0.6,
  1,
  2,
  3,
  4,
  6,
  9,
  1e-7,
  1e21,
  9007199254740991,
  9007199254740992,
  [],
  {},
];
const cases = [];
const add = (group, name, schema, inputs, override = false) =>
  cases.push({ group, name, schema, inputs, override });
for (const n of [0, 1, 2, 3]) {
  add("length", `min${n}`, z.string().min(n), strings);
  add("length", `max${n}`, z.string().max(n), strings);
  add("length", `len${n}`, z.string().length(n), strings);
}
for (const n of [0.1, 0.2, 0.3, 2, 3, 1e-7, 1e21, 0, -2])
  add("remainder", String(n), z.number().multipleOf(n), numbers);
add("remainder", "2-3", z.number().multipleOf(2).multipleOf(3), numbers);
add("remainder", "3-2", z.number().multipleOf(3).multipleOf(2), numbers);
for (const [name, regex] of [
  ["ignoreCase", /a/i],
  ["unicode", /^.$/u],
  ["plain", /^.$/],
  ["global", /a/g],
  ["sticky", /a/y],
  ["multiline", /^a$/m],
  ["dotall", /a.z/s],
])
  add("regex", name, z.string().regex(regex), strings);
add("regex", "repeated", z.string().regex(/^a/).regex(/z$/), strings);
for (const [name, schema] of [
  ["rawUUID", z.string().uuid()],
  ["rawEmail", z.string().email()],
  ["rawURL", z.string().url()],
  ["uuid-email", z.string().uuid().email()],
  ["email-uuid", z.string().email().uuid()],
  ["factoryEmail", validator.email],
  ["factoryEmail-uuid", validator.email.uuid()],
  ["factoryURI", validator.uri],
  ["factoryURI-uuid", validator.uri.uuid()],
  ["factoryEmail-repeat", validator.email.email()],
  ["factoryURI-repeat", validator.uri.url()],
])
  add("format", name, schema, strings);
add(
  "override",
  "length",
  z.string().min(4).openapi({ minLength: 1 }),
  strings,
  true,
);
add(
  "override",
  "multiple",
  z.number().multipleOf(2).multipleOf(3).openapi({ multipleOf: 1 }),
  numbers,
  true,
);
add(
  "override",
  "pattern",
  z.string().regex(/^a/).regex(/z$/).openapi({ pattern: "^x" }),
  strings,
  true,
);
add(
  "override",
  "format",
  validator.uri.openapi({ format: "email", pattern: undefined }),
  strings,
  true,
);

const rows = [];
let baselineCount = 0,
  exactCount = 0,
  nativeCount = 0;
for (const c of cases)
  for (const mode of ["baseline", "pure"]) {
    const schema =
      mode === "pure"
        ? validator.openapi2020(c.schema, { pureInput: true })
        : validator.openapi(c.schema);
    const Engine = mode === "pure" ? Ajv2020 : Ajv;
    const engine = new Engine({ strict: false });
    formats(engine);
    engine.addFormat("forklaunch-whatwg-url", (value) => {
      try {
        new URL(value);
        return true;
      } catch {
        return false;
      }
    });
    let baseline;
    try {
      baseline = engine.compile(schema);
    } catch {
      /* zero/negative ordinary multipleOf is an explicit baseline limitation */
    }
    for (const input of c.inputs) {
      baselineCount++;
      if (baseline)
        rows.push({
          group: c.group,
          mode,
          zod: validator.parse(c.schema, input).ok,
          ajv: !!baseline(input),
        });
    }
    // The opt-in only promises exact length/remainder; regex and mixed formats stay unchanged.
    const projected = validator.openapi2020(c.schema, {
      pureInput: mode === "pure",
      zodChecks: true,
    });
    const owned = registerZodChecks(new Ajv2020({ strict: false }));
    formats(owned);
    owned.addFormat("forklaunch-whatwg-url", (value) => {
      try {
        new URL(value);
        return true;
      } catch {
        return false;
      }
    });
    const validate = owned.compile(projected);
    const expected = c.inputs.map((input) => !!validate(input));
    if (c.group === "length" || c.group === "remainder")
      for (let i = 0; i < c.inputs.length; i++) {
        assert.equal(
          expected[i],
          validator.parse(c.schema, c.inputs[i]).ok,
          JSON.stringify({ name: c.name, mode, input: c.inputs[i], projected }),
        );
        exactCount++;
      }
    if (c.override)
      for (let i = 0; i < c.inputs.length; i++)
        assert.equal(
          expected[i],
          !!baseline(c.inputs[i]),
          "explicit override remains ordinary JSON Schema",
        );
    assert.deepEqual(
      validator.openapi2020(c.schema, {
        pureInput: mode === "pure",
        zodChecks: false,
      }),
      validator.openapi2020(c.schema, { pureInput: mode === "pure" }),
      "disabled profile leaves export unchanged",
    );
    if (native) {
      const actual = run(
        path.join(target, "debug/examples/uri_schema_parity"),
        [],
        {
          input:
            [projected, ...c.inputs]
              .map((value) => JSON.stringify(value))
              .join("\n") + "\n",
        },
      )
        .trim()
        .split("\n")
        .map(JSON.parse);
      if (c.group === "length" || c.group === "remainder")
        assert.deepEqual(actual, expected, c.name);
      nativeCount += actual.length;
    }
  }
for (const divisor of [NaN, Infinity, -Infinity, -0, -2, 1e-7, 1e21]) {
  const schema = z.number().multipleOf(divisor),
    projected = validator.openapi2020(schema, {
      pureInput: true,
      zodChecks: true,
    });
  const validate = registerZodChecks(new Ajv2020({ strict: false })).compile(
    projected,
  );
  const expected = numbers.map((value) => validator.parse(schema, value).ok);
  for (let i = 0; i < numbers.length; i++)
    assert.equal(!!validate(numbers[i]), expected[i], String(divisor));
  exactCount += numbers.length;
  if (native) {
    assert.deepEqual(
      run(path.join(target, "debug/examples/uri_schema_parity"), [], {
        input:
          [projected, ...numbers]
            .map((value) => JSON.stringify(value))
            .join("\n") + "\n",
      })
        .trim()
        .split("\n")
        .map(JSON.parse),
      expected,
    );
    nativeCount += numbers.length;
  }
}
const direct = z.string().length(2),
  numeric = z.number().multipleOf(0.1).multipleOf(0.3);
for (const base of [direct, numeric])
  for (const [schema, wrap] of [
    [base, (x) => x],
    [base.optional(), (x) => x],
    [base.nullable(), (x) => x],
    [z.array(base), (x) => [x]],
    [z.object({ value: base }), (x) => ({ value: x })],
    [z.record(z.string(), base), (x) => ({ x })],
    [z.tuple([base]), (x) => [x]],
    [z.union([base, z.literal("alternate")]), (x) => x],
  ]) {
    const exported = validator.openapi2020(schema, {
        pureInput: true,
        zodChecks: true,
      }),
      validate = registerZodChecks(new Ajv2020({ strict: false })).compile(
        exported,
      );
    const inputs = [...strings, ...numbers].map(wrap),
      expected = inputs.map((value) => validator.parse(schema, value).ok);
    inputs.forEach((value, i) => {
      assert.equal(
        !!validate(value),
        expected[i],
        JSON.stringify({ exported, value }),
      );
      exactCount++;
    });
    if (native) {
      assert.deepEqual(
        run(path.join(target, "debug/examples/uri_schema_parity"), [], {
          input:
            [exported, ...inputs]
              .map((value) => JSON.stringify(value))
              .join("\n") + "\n",
        })
          .trim()
          .split("\n")
          .map(JSON.parse),
        expected,
      );
      nativeCount += inputs.length;
    }
  }
// Explicit annotations, including own undefined, remain authoritative.
for (const override of [0, undefined, null]) {
  const schema = z.string().min(3).openapi({ minLength: override });
  const exported = validator.openapi2020(schema, { zodChecks: true });
  assert.equal(exported.minLength, override);
  assert.equal(exported["x-forklaunch-zod-checks"], undefined);
  const numeric = z.number().multipleOf(2).openapi({ multipleOf: override });
  assert.equal(
    validator.openapi2020(numeric, { zodChecks: true }).multipleOf,
    override,
  );
  assert.equal(
    validator.openapi2020(numeric, { zodChecks: true })[
      "x-forklaunch-zod-checks"
    ],
    undefined,
  );
}
assert.equal(
  validator.openapi2020(z.string().trim().min(3).openapi({ minLength: 1 }), {
    zodChecks: true,
  }).minLength,
  1,
);
assert.throws(
  () => validator.openapi2020(z.string().trim().min(3), { zodChecks: true }),
  /transformations/,
);
assert.deepEqual(
  validator.openapi2020(z.string().min(-1), { zodChecks: true })["x-forklaunch-zod-checks"],
  { version: 1, utf16MinLengths: [-1] },
);
assert.throws(
  () =>
    validator.openapi2020(
      z.string().openapi({ "x-forklaunch-zod-checks": { version: 1 } }),
      { zodChecks: true },
    ),
  /reserved/,
);
const keyword = "x-forklaunch-zod-checks";
for (const invalid of [
  null,
  [],
  {},
  { version: 2 },
  { version: 1, extra: 0 },
  { version: 1, utf16MinLength: -1 },
  { version: 1, utf16MaxLength: 0.5 },
  { version: 1, multipleOf: null },
  { version: 1, multipleOf: ["nan"] },
  { version: 1, multipleOf: Array(10001).fill(1) },
])
  assert.throws(
    () =>
      registerZodChecks(new Ajv2020({ strict: false })).compile({
        [keyword]: invalid,
      }),
    /Zod|object/,
  );
// JS can hold lone UTF-16 surrogates; Rust String cannot. This assertion is JS-only,
// not a claim that native JSON decoding admits that separate representation domain.
for (const value of ["\ud800", "\udfff", "a\ud800"]) {
  const schema = z.string().length(1),
    exported = validator.openapi2020(schema, { zodChecks: true });
  assert.equal(
    registerZodChecks(new Ajv2020({ strict: false })).compile(exported)(value),
    validator.parse(schema, value).ok,
  );
}
const socketRegistration = load(
  path.join(
    root,
    "framework-rs/crates/core/src/typescript/zod-checks.ts",
  ),
).registerZodChecks;
const sharedEngine = registerZodChecks(new Ajv2020({ strict: false }));
assert.throws(
  () => socketRegistration(sharedEngine),
  /already registered/,
  "separately bundled consumer identities require separate engines",
);
const wide = z.string();
wide._def.checks = Array(10001).fill({ kind: "min", value: 1 });
assert.throws(
  () => validator.openapi2020(wide, { zodChecks: true }),
  /budget|10000|limit/i,
);
let callbacks = 0;
const defaulted = z.object({
  value: z
    .string()
    .min(2)
    .default(() => {
      callbacks++;
      throw new Error("must not execute");
    }),
});
validator.openapi2020(defaulted, { pureInput: true, zodChecks: true });
assert.equal(callbacks, 0);
for (const metadata of [
  [{ minLength: 1 }, { minLength: undefined }],
  [{ minLength: undefined }, { minLength: 2 }],
]) {
  const value = metadata.reduce(
    (schema, item) => schema.openapi(item),
    z.string().min(3),
  );
  const result = validator.openapi2020(value, { zodChecks: true });
  assert.equal(result.minLength, metadata.at(-1).minLength);
  assert.equal(result["x-forklaunch-zod-checks"], undefined);
}
const partial = validator.openapi2020(
  z.string().length(2).openapi({ minLength: 0 }),
  { zodChecks: true },
);
assert.equal(partial.minLength, 0);
assert.deepEqual(partial["x-forklaunch-zod-checks"], {
  version: 1,
  utf16MaxLength: 2,
});
const foreign = new Ajv({ strict: false });
foreign.addKeyword({ keyword, validate: () => true });
assert.throws(() => registerZodChecks(foreign), /already registered/);
const injected = new Ajv2020({
  strict: false,
  coerceTypes: false,
  ownProperties: true,
});
injected.addKeyword({
  keyword: "applicationOnly",
  validate: (expected, value) => expected === value,
});
const originalOptions = { ...injected.opts };
registerZodChecks(injected);
registerZodChecks(injected);
assert.deepEqual(injected.opts, originalOptions);
assert.equal(injected.validate({ applicationOnly: "yes" }, "yes"), true);
assert.equal(injected.validate({ applicationOnly: "yes" }, "no"), false);
const { UniversalSdk } = load(
  path.join(root, "framework/universal-sdk/src/universalSdk.ts"),
);
let factories = 0;
const sdk = new UniversalSdk(
  "https://example.invalid",
  injected,
  {},
  undefined,
  () => {
    factories++;
    return new Ajv2020({ strict: false });
  },
);
const exported = validator.openapi2020(direct, { zodChecks: true });
const doc = {
  openapi: "3.1.0",
  info: { title: "Checks", version: "1" },
  paths: {},
  components: { schemas: { Check: exported } },
};
const validate = sdk.responseSchemas.validator(
  doc,
  "/components/schemas/Check",
  { $ref: "#/components/schemas/Check" },
);
assert.equal(validate("😀"), true);
assert.equal(validate("a"), false);
assert.equal(factories, 1);
console.log(
  `PASS ${baselineCount} baseline observations (known regex/format/legacy remainder gaps retained), ${exactCount} exact opted-in actual Zod/AJV cases, ${nativeCount} native observations; nested/pure/override/injected/configuration controls`,
);
