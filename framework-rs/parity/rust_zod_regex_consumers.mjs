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
assert.equal(process.versions.node,"24.2.0","Pinned regex semantic target");
const source=z.object({text:z.string().regex(/^[ab]+z$/g).regex(/a/),sticky:z.string().regex(/a/y)}).strict();
const schema=validator.openapi2020(source,{pureInput:true,zodRegex:true});
const cases=[{text:"abz",sticky:"a"},{text:"az",sticky:"ab"},{text:"bz",sticky:"a"},{text:"abz",sticky:"ba"},{text:"abz\n",sticky:"a"},{text:"a😀z",sticky:"a"}];
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
const work = mkdtempSync(path.join(tmpdir(), "fl-zod-regex-consumers-"));
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
    sdk = await universalSdk({ host, registryOptions: { raw: doc }, zodRegexExecution:"host-native" });
  for (let i = 0; i < cases.length; i++) {
    payload = cases[i];
    if (expected[i])
      assert.deepEqual((await sdk.value()).response, {
        ...payload,
      });
    else await assert.rejects(() => sdk.value());
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
  assert.match(denied.stderr, /zod-regex feature/);
  run("cargo", [...build, "--features", "zod-regex"]);
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
  assert.throws(()=>new SocketCodec(schema,4096),/host-native/);
  const codec = new SocketCodec(schema, 4096, false, false, "host-native");
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
    `[package]\nname="fl-zod-regex-consumers"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nforklaunch-ws={path=${JSON.stringify(path.join(root, "framework-rs/crates/ws"))},features=["transport","json-schema","zod-regex"]}\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\ntokio={version="1",features=["net"]}\ntokio-tungstenite="0.30"\n`,
  );
  writeFileSync(path.join(work, "src/lib.rs"), generated.rust.socket_checks);
  writeFileSync(
    path.join(work, "src/main.rs"),
    `use fl_zod_regex_consumers::SocketClient;fn main(){let codecs=SocketClient::schemas().unwrap();for (wire,valid) in [${cases.map((value, i) => `(${JSON.stringify(JSON.stringify(value))},${expected[i]})`).join(",")}]{let decoded=codecs.incoming.decode(wire.as_bytes());assert_eq!(decoded.is_ok(),valid);let value=serde_json::from_str(wire).unwrap();assert_eq!(codecs.outgoing.encode(&value).is_ok(),valid);}println!("PASS generated Rust checked-regex codecs");}`,
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
    `PASS ${requests} live owned UniversalSdk responses, ${cases.length * 4} generated TS/Rust socket codec checks; strict generated sources and feature-off compiler fails before output`,
  );
} finally {
  if (server) await new Promise((resolve) => server.close(resolve));
  rmSync(work, { recursive: true, force: true });
}
