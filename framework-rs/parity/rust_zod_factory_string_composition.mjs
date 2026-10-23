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
const { validator, statics, load } = actualValidator(root, undefined, {
  sameRealm: true,
});
const require = createRequire(
  path.join(
    process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES ??
      "/private/tmp/fl-canonical-http-dependencies",
    "package.json",
  ),
);
const { z } = require("zod/v3"),
  Ajv = require("ajv"),
  Ajv2020 = require("ajv/dist/2020").default,
  addFormats = require("ajv-formats");
const { generateSchema } = load(
  path.join(root, "framework/validator/shims/zod-v3-openapi/zod-openapi.ts"),
);
const { registerFactoryUriFormat } = load(
  path.join(root, "framework/universal-sdk/src/core/uriFormat.ts"),
);
assert.equal(process.versions.node, "24.2.0");
assert.equal(process.versions.ada, "3.2.4");
const inputs = [
  null,
  0,
  false,
  [],
  {},
  "",
  "a'b@example.com",
  "a'@example.com",
  "a@x-.com",
  "a@example.com",
  "a@example.com\n",
  "a%b@example.com",
  "a@b.co",
  "http://example.com/a b",
  "https://bücher.de",
  "http://256.256.256.256",
  "http://example.com\n",
  "http://example.com/",
  "mailto:a@b.com",
  "a:xx",
  "http://a\u061db.test",
  "other",
];
const shapes = [
  (x) => x,
  (x) => z.object({ value: x }),
  (x) => z.array(x),
  (x) => x.optional(),
  (x) => z.union([x, z.literal("other")]),
];
let comparisons = 0,
  exports = 0,
  controls = 0;
const native = [];
for (const [name, base, kind, literal] of [
  ["email", statics.email, "email", "@"],
  ["uri", statics.uri, "url", "/"],
]) {
  const leaves = [
    base.min(1),
    base.max(200),
    base.length(10),
    base.includes(literal),
    base.startsWith(name === "email" ? "a" : "http"),
    base.endsWith(name === "email" ? ".com" : "/"),
    base[kind](),
    base.min(1).includes(literal).max(200),
    base.min(50).max(2),
    base.min(10).min(1),
  ];
  for (const leaf of leaves)
    for (let w = 0; w < shapes.length; w++)
      for (const profile of ["3.0", "3.1", "pure"]) {
        const schema = shapes[w](leaf),
          doc =
            profile === "pure"
              ? validator.openapi2020(schema, { pureInput: true })
              : generateSchema(schema, false, profile);
        const engine = registerFactoryUriFormat(
          new (profile === "pure" ? Ajv2020 : Ajv)({ strict: false }),
        );
        addFormats(engine);
        const check = engine.compile(doc);
        exports++;
        assert.ok(
          name === "email"
            ? !JSON.stringify(doc).includes('"format":"email"')
            : JSON.stringify(doc).includes("forklaunch-whatwg-url"),
        );
        const cases = inputs.map((value) => {
          const input = w === 1 ? { value } : w === 2 ? [value] : value,
            valid = schema.safeParse(input).success;
          assert.equal(
            Boolean(check(input)),
            valid,
            JSON.stringify({ name, input, doc }),
          );
          comparisons++;
          return { input, valid };
        });
        if (profile === "3.1") native.push({ schema: doc, cases, name });
      }
  for (const schema of [
    base.trim().min(1),
    base.regex(/x/),
    base.includes("x", { position: 1 }),
  ]) {
    const doc = validator.openapi(schema);
    assert.equal(doc.format, name === "email" ? "email" : "uri");
    controls++;
  }
  const overridden = validator.openapi(
    base.min(1).openapi({ format: "custom-format", pattern: "^custom$" }),
  );
  assert.equal(overridden.format, "custom-format");
  assert.equal(overridden.pattern, "^custom$");
  controls++;
  let calls = 0;
  validator.openapi2020(
    z.object({
      value: base.min(1).default(() => {
        calls++;
        throw Error("no callback");
      }),
    }),
    { pureInput: true },
  );
  assert.equal(calls, 0);
  controls++;
}
assert.equal(validator.openapi(z.string().email().min(1)).format, "email");
assert.equal(validator.openapi(z.string().url().min(1)).format, "uri");
controls += 2;
const custom = new Ajv({ strict: false });
custom.addFormat("kept", (x) => x === "custom");
assert.equal(registerFactoryUriFormat(custom), custom);
assert.equal(
  custom.compile({ type: "string", format: "kept" })("custom"),
  true,
);
assert.equal(
  custom.compile({ type: "string", format: "kept" })("other"),
  false,
);
controls++;
console.log(
  `PASS ${comparisons} actual factory Zod/AJV composition comparisons across ${exports} exports +${controls} raw/unsupported/custom/injected/callback controls`,
);
if (process.env.FORKLAUNCH_FACTORY_STRING_NATIVE === "1") {
  const env = {
      ...process.env,
      CARGO_INCREMENTAL: "0",
      CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "1",
    },
    target = path.resolve(
      process.env.CARGO_TARGET_DIR ?? path.join(root, "framework-rs/target"),
    );
  function build(features) {
    const result = spawnSync(
      "cargo",
      [
        "build",
        "--locked",
        "--manifest-path",
        "framework-rs/Cargo.toml",
        "-p",
        "forklaunch-core",
        "--features",
        features,
        "--example",
        "uri_schema_parity",
      ],
      { cwd: root, env, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
    );
    assert.equal(result.status, 0, result.stderr);
  }
  function execute(schema, values) {
    return spawnSync(
      path.join(target, "debug/examples/uri_schema_parity"),
      [],
      {
        cwd: root,
        env,
        encoding: "utf8",
        input:
          [schema, ...values].map((x) => JSON.stringify(x)).join("\n") + "\n",
      },
    );
  }
  build("schema-validation");
  const off = execute(native.find((x) => x.name === "uri").schema, [
    "http://example.com",
  ]);
  assert.notEqual(off.status, 0);
  assert.match(off.stderr, /requires the forklaunch-core zod-uri feature/);
  build("zod-uri");
  let count = 0;
  for (const { schema, cases } of native) {
    const result = execute(
      schema,
      cases.map((x) => x.input),
    );
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(
      result.stdout
        .trim()
        .split("\n")
        .map((x) => x === "true"),
      cases.map((x) => x.valid),
      JSON.stringify(schema),
    );
    count += cases.length;
  }
  console.log(
    `PASS ${count} shipped native factory composition comparisons across ${native.length} schemas + feature-off rejection; pinned Ada3.2.4`,
  );
}
