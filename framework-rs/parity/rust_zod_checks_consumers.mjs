import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  symlinkSync,
  rmSync,
} from "node:fs";
import { createRequire } from "node:module";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { actualValidator } from "./rust_actual_validator_fixture.mjs";
import { actualUniversalSdk } from "./fixtures/rust_better_auth_frontend_client.mjs";
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const deps = process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES;
const require = createRequire(path.join(deps, "package.json"));
const { z } = require("zod/v3");
const { validator } = actualValidator(root);
const source = z
  .object({
    anyPayload: validator.any,
    unknownPayload: validator.unknown,
    typedPayload: validator.type(),
    text: z.string().length(2),
    floatText: z.string().min(1.5).max(2.5),
    nanText: z.string().length(NaN),
    fragments: z.string().startsWith("\ud83d").endsWith("\ude00").includes("\ude00",{position:1.5}),
    positioned: z.string().includes("a",{position:1}),
    dispatch: z.discriminatedUnion("tag", [z.object({tag:z.literal("a").default("a")}),z.object({tag:z.literal("b")})]),
    keyed: z.record(z.string().min(2).max(3), z.string()),
    cardinality: z.array(z.string()).min(1.5).max(2.5).length(NaN),
    number: z.number().multipleOf(0.1),
    finiteBound: z.number().min(NaN).min(-1).gt(-2).max(Infinity),
    impossibleBound: z.number().min(Infinity).min(-1).optional(),
    nullableBound: z.number().max(-Infinity).nullable(),
    email: z.string().email().email(),
    uuid: z.string().uuid(),
    uri: z.string().url().url(),
    impossible: z.string().email().uuid().optional(),
    cuid: z.string().cuid(),
    cuid2: z.string().cuid2(),
    ulid: z.string().ulid(),
    nanoid: z.string().nanoid(),
    base64: z.string().base64(),
    base64url: z.string().base64url(),
    date: z.string().date(),
    duration: z.string().duration(),
    time: z.string().time(),
    preciseTime: z.string().time({precision:2}),
    datetime: z.string().datetime({offset:true,local:true,precision:2}),
    ip: z.string().ip({version:"v6"}),
    cidr: z.string().cidr({version:"v4"}),
    emoji: z.string().emoji(),
    jwt: z.string().jwt({alg:"HS256"}),
  })
  .strict();
const schema = validator.openapi2020(source, {
  pureInput: true,
  zodChecks: true,
});
const cases = [
  { text: "😀", number: 0.3 },
  { text: "ab", number: 0.6 },
  { text: "a", number: 0.3 },
  { text: "😀a", number: 0.3 },
  { text: "😀", number: 0.1 + 0.2 },
];
const validFormats = {
  floatText: "😀",
  nanText: "anything",
  fragments: "😀",
  positioned: "ba",
  nullableBound: null,
  finiteBound: 0,
  dispatch: {tag:"a"},
  keyed: {"😀": "value", "ab": "value"},
  cardinality:["a","b"],
  email: "a'b@b-.co",
  uuid: "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
  uri: "http://example.test/a b",
  cuid: "c😀😀😀😀",
  cuid2: "lower123",
  ulid: "z".repeat(26),
  nanoid: "-".repeat(21),
  base64: "AB==",
  base64url: "_w",
  date: "0000-02-29",
  duration: "-P1Y2DT3H4M5,5S",
  time: "23:59",
  preciseTime: "00:00:00.12:59.34",
  datetime: "0000-02-29T00:00:00.12+99:99",
  ip: "fe80:::::%eth0",
  cidr: "1.2.3.4/32",
  emoji: "#0123😀",
  jwt: Buffer.from(JSON.stringify({alg:"HS256",typ:"JWT"})).toString("base64url")+".x.",
};
for (const value of cases) Object.assign(value, validFormats);
for (const patch of [
  {floatText:"a"},
  {floatText:"a😀"},
  {nanText:""},
  {fragments:"a😀"},
  {fragments:"😀a"},
  {positioned:"ab"},
  {nullableBound:2},
  {finiteBound:-1},
  {finiteBound:-1.5},
  {impossibleBound:0},
  {dispatch:{}},
  {dispatch:{tag:"b"}},
  {dispatch:{tag:"unknown"}},
  {keyed: {a:"value"}},
  {keyed: {"😀😀":"value"}},
  {keyed: {}},
  {cardinality:[]},
  {cardinality:["a","b","c"]},
  {cardinality:[{},"b"]},
  { email: "a%b@b.co" },
  { uuid: "urn:uuid:" + validFormats.uuid },
  { uri: "http://256.1.1.1" },
  { impossible: "a@b.co" },
  { cuid: "c😀😀😀" },
  { cuid2: "Upper" },
  { ulid: "I".repeat(26) },
  { nanoid: "a".repeat(20) },
  { base64: "AA" },
  { base64url: "/w==" },
  { date: "1900-02-29" },
  { duration: "P1.5Y1M" },
  { time: "24:00" },
  { preciseTime: "00:00:00.1" },
  { datetime: "1900-02-29T00:00:00.12Z" },
  { ip: "::FFFF:192.168.1.1" },
  { cidr: "1.2.3.4/033" },
  { emoji: "😀a" },
  { jwt: Buffer.from(JSON.stringify({alg:0})).toString("base64url")+".x." },
  { jwt: Buffer.from(JSON.stringify({alg:"RS256"})).toString("base64url")+".x." },
])
  cases.push({ text: "😀", number: 0.3, ...validFormats, ...patch });
// Factory-generated unconstrained schemas must preserve scalar and aggregate values
// through both generated socket codecs, as well as the actual HTTP SDK.
for (const value of cases) Object.assign(value, {anyPayload: null, unknownPayload: false, typedPayload: 0});
for (const value of [null, false, true, 0, 1.5, "scalar", [], [null, 1, "x"], {}, {nested: [false, {value: "x"}]}]) {
  cases.push({text: "😀", number: 0.3, ...validFormats, anyPayload: value, unknownPayload: value, typedPayload: value});
}
assert.equal(
  process.versions.node,
  "24.2.0",
  "URI acceptance oracle pins Node/Ada",
);
assert.equal(process.versions.ada, "3.2.4");
const expected = cases.map((value) => validator.parse(source, value).ok);
const env = {
  ...process.env,
  CARGO_INCREMENTAL: "0",
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "1",
};
const target = path.resolve(
  process.env.CARGO_TARGET_DIR ?? path.join(root, "framework-rs/target"),
);
function invoke(command, args, options = {}) {
  return spawnSync(command, args, {
    cwd: root,
    env,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    ...options,
  });
}
function run(command, args, options = {}) {
  const result = invoke(command, args, options);
  assert.equal(result.status, 0, result.stdout + "\n" + result.stderr);
  return result.stdout;
}
const work = mkdtempSync(path.join(tmpdir(), "fl-zod-check-consumers-"));
let server;
try {
  const { universalSdk } = await actualUniversalSdk();
  let payload,
    requests = 0;
  server = http.createServer((_, res) => {
    requests++;
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify(payload));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const host = `http://127.0.0.1:${server.address().port}`;
  const doc = {
    openapi: "3.1.0",
    info: { title: "Checks", version: "1" },
    paths: {
      "/value": {
        get: {
          operationId: "value",
          responses: { 200: { content: { "application/json": { schema } } } },
        },
      },
    },
  };
  const before = JSON.stringify(doc),
    sdk = await universalSdk({ host, registryOptions: { raw: doc } });
  for (let i = 0; i < cases.length; i++) {
    payload = cases[i];
    if (expected[i])
      assert.deepEqual((await sdk.value()).response, {
        impossible: undefined,
        ...payload,
      });
    else await assert.rejects(() => sdk.value(), JSON.stringify({index:i,payload}));
  }
  assert.equal(requests, cases.length);
  assert.equal(JSON.stringify(doc), before);
  const artifact = path.join(work, "artifact.json");
  writeFileSync(
    artifact,
    JSON.stringify({
      formatVersion: 1,
      title: "Checks",
      version: "1",
      sockets: {
        checks: {
          path: "/checks",
          clientMessages: { request: { shape: schema } },
          serverMessages: { response: { shape: schema } },
        },
      },
    }),
  );
  const build = [
    "build",
    "--locked",
    "--manifest-path",
    "framework-rs/Cargo.toml",
    "-p",
    "forklaunch-core","--features","build,asyncapi-import",
    "--example",
    "typescript_socket_artifacts",
  ];
  run("cargo", build);
  const binary = path.join(target, "debug/examples/typescript_socket_artifacts");
  const denied = invoke(binary, [artifact, "all"]);
  assert.notEqual(denied.status, 0);
  assert.equal(denied.stdout, "");
  assert.match(denied.stderr, /zod-checks feature/);
  run("cargo", [...build, "--features", "zod-checks,zod-uri"]);
  const generated = JSON.parse(run(binary, [artifact, "all"]));
  mkdirSync(path.join(work, "src"));
  symlinkSync(
    path.join(
      process.env.FORKLAUNCH_TEST_SOCKET_DEPENDENCIES ??
        "/private/tmp/fl-binary-socket-dependencies",
      "node_modules",
    ),
    path.join(work, "node_modules"),
    "dir",
  );
  writeFileSync(
    path.join(work, "package.json"),
    JSON.stringify({ type: "module" }),
  );
  writeFileSync(
    path.join(work, "src/client.ts"),
    generated.typescript.socket_checks + "\nexport {SocketCodec};\n",
  );
  writeFileSync(
    path.join(work, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        strict: true,
        target: "ES2022",
        module: "NodeNext",
        moduleResolution: "NodeNext",
        esModuleInterop: true,
        types: ["node"],
        outDir: "dist",
        skipLibCheck: false,
      },
      include: ["src/*.ts"],
    }),
  );
  run(process.env.TSC_BIN ?? "tsc", ["-p", path.join(work, "tsconfig.json")]);
  const { SocketCodec } = await import(
    pathToFileURL(path.join(work, "dist/client.js"))
  );
  const codec = new SocketCodec(schema, 4096);
  for (let i = 0; i < cases.length; i++) {
    const bytes = Buffer.from(JSON.stringify(cases[i]));
    if (expected[i]) {
      assert.deepEqual(codec.decode(bytes), cases[i]);
      assert.deepEqual(JSON.parse(codec.encode(cases[i]).toString()), cases[i]);
    } else {
      assert.throws(() => codec.decode(bytes));
      assert.throws(() => codec.encode(cases[i]));
    }
  }
  writeFileSync(
    path.join(work, "Cargo.toml"),
    `[package]\nname="fl-zod-check-consumers"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nforklaunch-ws={path=${JSON.stringify(path.join(root, "framework-rs/crates/ws"))},features=["transport","json-schema","zod-checks","zod-uri"]}\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\ntokio={version="1",features=["net"]}\ntokio-tungstenite="0.30"\n`,
  );
  writeFileSync(path.join(work, "src/lib.rs"), generated.rust.socket_checks);
  writeFileSync(
    path.join(work, "src/main.rs"),
    `use fl_zod_check_consumers::SocketClient;fn main(){let codecs=SocketClient::schemas().unwrap();for (wire,valid) in [${cases.map((value, i) => `(${JSON.stringify(JSON.stringify(value))},${expected[i]})`).join(",")}]{let decoded=codecs.incoming.decode(wire.as_bytes());assert_eq!(decoded.is_ok(),valid);match serde_json::from_str(wire){Ok(value)=>assert_eq!(codecs.outgoing.encode(&value).is_ok(),valid),Err(_)=>assert!(!valid,"valid payload must construct generated type")};}println!("PASS generated Rust exact-check codecs");}`,
  );
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
    "run",
    "--offline",
    "--quiet",
    "--manifest-path",
    path.join(work, "Cargo.toml"),
  ]);
  console.log(
    `PASS ${requests} live owned UniversalSdk responses, ${cases.length * 4} generated TS/Rust socket codec/type-admission checks; strict generated sources and feature-off compiler fails before output`,
  );
} finally {
  if (server) await new Promise((resolve) => server.close(resolve));
  rmSync(work, { recursive: true, force: true });
}
