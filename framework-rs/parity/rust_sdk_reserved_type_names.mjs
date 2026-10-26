import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const env = {
  ...process.env,
  CARGO_INCREMENTAL: "0",
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "1",
};
const target = path.resolve(
  process.env.CARGO_TARGET_DIR ?? path.join(root, "framework-rs/target"),
);
function run(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, {
    cwd: root,
    env,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    ...options,
  });
  assert.equal(
    result.status,
    0,
    `${cmd} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`,
  );
  return result.stdout;
}
const dialect = "https://json-schema.org/draft/2020-12/schema";
const fixtures = [
  {
    schema: {
      type: "object",
      properties: { name: { type: "string" } },
      required: ["name"],
      additionalProperties: false,
    },
    valid: [{ name: "x" }],
    invalid: [{}, null, { name: 1 }],
  },
  { schema: { type: "string" }, valid: ["x"], invalid: [null, 1, {}] },
  {
    schema: {
      $schema: dialect,
      type: "array",
      prefixItems: [{ type: "string" }],
      items: false,
      minItems: 1,
    },
    valid: [["x"]],
    invalid: [[], [1], null],
  },
  {
    schema: {
      $schema: dialect,
      type: "array",
      prefixItems: [{ $ref: "#" }],
      items: { $ref: "#" },
      maxItems: 2,
    },
    valid: [[], [[]], [[[]]], [[], []]],
    invalid: [[1], null, [[], [], []]],
  },
  {
    schema: {
      $schema: dialect,
      $defs: {
        Self: {
          type: "object",
          properties: { next: { $ref: "#/$defs/Self" } },
          additionalProperties: false,
        },
      },
      $ref: "#/$defs/Self",
    },
    valid: [{}, { next: {} }, { next: { next: {} } }],
    invalid: [null, { next: 1 }],
  },
];
run("cargo", [
  "build",
  "--locked",
  "--manifest-path",
  "framework-rs/Cargo.toml",
  "-p",
  "forklaunch-core","--features","build,asyncapi-import",
  "--example",
  "codegen_fixed_tuple_models",
]);
let count = 0;
const modules = [],
  types = [];
for (const name of ["Self", "self", "SELF", "Self_", "ModelSelf", "Selfish"]) {
  const generated = JSON.parse(
    run(path.join(target, "debug/examples/codegen_fixed_tuple_models"), [name], {
      input: JSON.stringify(fixtures.map((x) => x.schema)),
    }),
  );
  for (let i = 0; i < fixtures.length; i++) {
    const item = generated[i];
    assert.equal(
      item.root,
      ["ModelSelf", "Selfish"].includes(name) ? name : "Self_",
    );
    const checks = [
      ...fixtures[i].valid.map((value) => ({ value, valid: true })),
      ...fixtures[i].invalid.map((value) => ({ value, valid: false })),
    ]
      .map(({ value, valid }) => {
        count++;
        return `{let wire=${JSON.stringify(JSON.stringify(value))};let decoded=serde_json::from_str::<${item.root}>(wire);assert_eq!(decoded.is_ok(),${valid});${valid ? "assert_eq!(serde_json::to_value(decoded.unwrap()).unwrap(),serde_json::from_str::<serde_json::Value>(wire).unwrap());" : ""}}`;
      })
      .join("\n");
    types.push(
      `namespace Case${types.length}{${Object.entries(item.definitions)
        .map(([key, value]) => `export type ${key}=${value};`)
        .join(
          "\n",
        )}export type Value=${item.typescript};${fixtures[i].valid.map((value, index) => `const valid${index}:Value=${JSON.stringify(value)};`).join("\n")}}`,
    );
    modules.push(
      `pub mod case${modules.length}{${item.source}\n#[test]fn roundtrip(){${checks}}}`,
    );
  }
}
const work = mkdtempSync(path.join(tmpdir(), "fl-reserved-type-names-"));
try {
  mkdirSync(path.join(work, "src"));
  writeFileSync(
    path.join(work, "Cargo.toml"),
    '[package]\nname="fl-reserved-type-names"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\n',
  );
  writeFileSync(path.join(work, "src/lib.rs"), modules.join("\n"));
  // Offline builds need every resolved package, including other targets' dependencies a host build never downloads; fetch them first.
  run("cargo", ["fetch", "--manifest-path", path.join(work, "Cargo.toml")]);
  run("cargo", [
    "rustc",
    "--offline",
    "--manifest-path",
    path.join(work, "Cargo.toml"),
    "--lib",
    "--",
    "-D",
    "warnings",
  ]);
  run("cargo", [
    "test",
    "--offline",
    "--manifest-path",
    path.join(work, "Cargo.toml"),
  ]);
  writeFileSync(path.join(work, "types.ts"), types.join("\n"));
  run(process.env.TSC_BIN ?? "tsc", [
    "--noEmit",
    "--strict",
    "--skipLibCheck",
    "--target",
    "es2022",
    path.join(work, "types.ts"),
  ]);
  console.log(
    `PASS ${count} generated reserved-name acceptance/roundtrips across ${modules.length} object/alias/fixed/recursive tuple/reference models; warnings denied, Self_ idempotence and ordinary names preserved`,
  );
} finally {
  rmSync(work, { recursive: true, force: true });
}
