import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { actualValidator } from './rust_actual_validator_fixture.mjs';

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
);
const frameworkRoot = path.resolve(
  root,
  process.env.FORKLAUNCH_TEST_FRAMEWORK_ROOT ?? 'framework-rs',
);
const temporary = await mkdtemp(path.join(tmpdir(), 'fl-ws-generated-sdk-'));
const target = path.resolve(
  root,
  process.env.CARGO_TARGET_DIR ?? 'framework-rs/target',
);
const env = {
  ...process.env,
  CARGO_TARGET_DIR: target,
  CARGO_INCREMENTAL: '0',
};
const sockets = new Set();
let server, child;
try {
  await mkdir(path.join(temporary, 'src'));
  execFileSync(
    'cargo',
    [
      'test',
      '--quiet',
      '--manifest-path',
      path.join(frameworkRoot, 'Cargo.toml'),
      '-p',
      'forklaunch-core','--features','build,asyncapi-import',
      'generated_socket_client_fixture',
    ],
    {
      cwd: root,
      env: {
        ...env,
        FORKLAUNCH_TEST_RUST_WS_SDK_OUTPUT: path.join(temporary, 'src/lib.rs'),
      },
      stdio: 'inherit',
      timeout: 300000,
    },
  );
  await writeFile(
    path.join(temporary, 'Cargo.toml'),
    `[package]
name = "forklaunch-ws-generated-fixture"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
forklaunch-ws = { path = ${JSON.stringify(path.join(frameworkRoot, 'crates/ws'))}, features = ["transport", "json-schema"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["net", "macros", "rt-multi-thread"] }
tokio-tungstenite = "0.30"
`,
  );
  await writeFile(
    path.join(temporary, 'src/main.rs'),
    `
use forklaunch_ws_generated_fixture::*;
use forklaunch_ws::transport::{IntoClientRequest, SocketEvent, SocketOptions};
#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).unwrap();
    assert!(ForkLaunchClient::connect(url.as_str(), SocketOptions::default()).await.is_err());
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert("authorization", "Bearer fixture-token".parse().unwrap());
    request.headers_mut().insert("sec-websocket-protocol", "chat.v1".parse().unwrap());
    let (mut client, response) = ForkLaunchClient::connect(request, SocketOptions::default()).await.unwrap();
    assert_eq!(response.headers()["sec-websocket-protocol"], "chat.v1");
    assert!(client.send(&SocketOutgoing { message: String::new() }).await.is_err());
    assert!(client.send(&SocketOutgoing { message: "x".repeat(33) }).await.is_err());
    assert!(client.ping(Some(&SocketPing { ts: 0 })).await.is_err());
    client.send(&SocketOutgoing { message: "hello 水 🦀".into() }).await.unwrap();
    if std::env::args().nth(2).as_deref() == Some("invalid") {
        assert!(matches!(client.recv().await, Err(forklaunch_ws::transport::SocketError::Payload(_))));
        assert!(client.recv().await.unwrap().is_none());
        println!("PASS generated Rust WebSocket SDK rejection");
        return;
    }
    match client.recv().await.unwrap().unwrap() {
        SocketEvent::Message(value) => assert_eq!(value.message, "hello 水 🦀"),
        other => panic!("unexpected event: {other:?}"),
    }
    client.ping(Some(&SocketPing { ts: 42 })).await.unwrap();
    match client.recv().await.unwrap().unwrap() {
        SocketEvent::Pong(value) => assert_eq!(value.ts, 42),
        other => panic!("unexpected event: {other:?}"),
    }
    client.close(1000, Some(&SocketClose { reason: "done".into() })).await.unwrap();
    match client.recv().await.unwrap().unwrap() {
        SocketEvent::Close { code, reason } => { assert_eq!(code, 1000); assert_eq!(reason.reason, "done"); },
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(client.recv().await.unwrap().is_none());
    println!("PASS generated Rust WebSocket SDK runtime");
}
`,
  );
  execFileSync(
    'cargo',
    [
      'rustc',
      '--quiet',
      '--manifest-path',
      path.join(temporary, 'Cargo.toml'),
      '--lib',
      '--',
      '-D',
      'warnings',
    ],
    { cwd: root, env, stdio: 'inherit', timeout: 300000 },
  );
  execFileSync(
    'cargo',
    ['build', '--quiet', '--manifest-path', path.join(temporary, 'Cargo.toml')],
    { cwd: root, env, stdio: 'inherit', timeout: 300000 },
  );
  for (const suffix of ['recursive', 'empty', 'dictionary']) {
    const directory = path.join(temporary, suffix);
    await mkdir(directory);
    const manifest = (
      await readFile(path.join(temporary, 'Cargo.toml'), 'utf8')
    )
      .replace(
        'forklaunch-ws-generated-fixture',
        `forklaunch-ws-${suffix}-fixture`,
      )
      .replace(
        '[workspace]',
        `[lib]\npath = "../src/lib.rs.${suffix}"\n[workspace]`,
      );
    await writeFile(path.join(directory, 'Cargo.toml'), manifest);
    execFileSync(
      'cargo',
      [
        'rustc',
        '--quiet',
        '--manifest-path',
        path.join(directory, 'Cargo.toml'),
        '--lib',
        '--',
        '-D',
        'warnings',
      ],
      { cwd: root, env, stdio: 'inherit', timeout: 300000 },
    );
  }
  if (process.argv.includes('--compile-only')) {
    console.log(
      'PASS generated native WebSocket consumers: normal, recursive, empty and dictionary clients compile with warnings denied',
    );
  } else {
    const {
      statics: v,
      validator,
      load,
    } = actualValidator(root, (spec, file, load) =>
      spec === '@forklaunch/core/ws'
        ? load(path.join(root, 'framework/core/src/ws/webSocketLike.ts'))
        : undefined,
    );
    const { ForklaunchWebSocketServer } = load(
      path.join(root, 'framework/ws/src/webSocketServer.ts'),
    );
    const external = createRequire(
      path.join(
        process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,
        'package.json',
      ),
    );
    const { WebSocketServer } = external('ws');
    const schemas = {
      clientMessages: {
        chat: { shape: v.schemify({ message: v.string.min(1).max(32) }) },
      },
      serverMessages: {
        chat: { shape: v.schemify({ message: v.string.min(1).max(32) }) },
      },
      ping: { shape: v.schemify({ ts: v.number.innerType().int().min(1) }) },
      pong: { shape: v.schemify({ ts: v.number.innerType().int().min(1) }) },
      closeReason: { reason: { shape: v.schemify({ reason: v.string }) } },
    };
    server = new ForklaunchWebSocketServer(validator, schemas, {
      host: '127.0.0.1',
      port: 0,
      path: '/chat',
      handleProtocols: (protocols) =>
        protocols.has('chat.v1') ? 'chat.v1' : false,
      verifyClient: ({ req }, done) =>
        done(
          req.headers.authorization === 'Bearer fixture-token',
          401,
          'Unauthorized',
        ),
    });
    let messageCount = 0,
      pingCount = 0;
    const closed = new Promise((resolve, reject) => {
      server.on('error', reject);
      server.on('connection', (socket) => {
        sockets.add(socket);
        socket.on('error', reject);
        socket.on('message', (value) => {
          try {
            messageCount++;
            assert.equal(value.message, 'hello 水 🦀');
            socket.send(value);
          } catch (error) {
            reject(error);
          }
        });
        socket.on('ping', (value) => {
          try {
            pingCount++;
            assert.equal(value.ts, 42);
          } catch (error) {
            reject(error);
          }
        });
        socket.on('close', (code, reason) => {
          try {
            assert.equal(code, 1000);
            assert.equal(reason.reason, 'done');
            resolve();
          } catch (error) {
            reject(error);
          }
        });
      });
    });
    closed.catch(() => {});
    await once(server, 'listening');
    const output = await runPeer(
      `ws://127.0.0.1:${server.address().port}/chat`,
      closed,
    );
    assert.match(output, /PASS generated Rust WebSocket SDK runtime/);
    assert.equal(messageCount, 1);
    assert.equal(pingCount, 1);
    await new Promise((resolve) => server.close(resolve));
    server = undefined;
    for (const invalid of [
      { message: '' },
      { message: 'valid', extra: true },
      { message: 42 },
    ]) {
      server = new WebSocketServer({
        host: '127.0.0.1',
        port: 0,
        path: '/chat',
        verifyClient: ({ req }, done) =>
          done(
            req.headers.authorization === 'Bearer fixture-token',
            401,
            'Unauthorized',
          ),
      });
      const rejected = new Promise((resolve, reject) => {
        server.on('error', reject);
        server.on('connection', (socket) => {
          sockets.add(socket);
          socket.on('error', reject);
          socket.once('message', () => socket.send(JSON.stringify(invalid)));
          socket.once('close', (code) => {
            try {
              assert.equal(code, 1008);
              resolve();
            } catch (error) {
              reject(error);
            }
          });
        });
      });
      rejected.catch(() => {});
      await once(server, 'listening');
      const result = await runPeer(
        `ws://127.0.0.1:${server.address().port}/chat`,
        rejected,
        'invalid',
      );
      assert.match(result, /PASS generated Rust WebSocket SDK rejection/);
      await new Promise((resolve) => server.close(resolve));
      server = undefined;
    }
    console.log(
      'PASS generated native Rust WebSocket client against actual TypeScript server: warning-free compilation, authorization, subprotocols, schema limits, Unicode, typed ping/automatic pong and close handshake; invalid constraints/types/unknown fields close with 1008 before delivery',
    );
  }
} finally {
  if (child && child.exitCode === null) child.kill();
  for (const socket of sockets) if (socket.readyState !== 3) socket.terminate();
  if (server) await new Promise((resolve) => server.close(resolve));
  await rm(temporary, { recursive: true, force: true });
}

async function runPeer(url, closed, mode = 'normal') {
  child = spawn(
    path.join(target, 'debug/forklaunch-ws-generated-fixture'),
    [url, mode],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );
  let output = '',
    errors = '',
    timer;
  child.stdout.on('data', (data) => (output += data));
  child.stderr.on('data', (data) => (errors += data));
  const finished = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code) =>
      code === 0
        ? resolve()
        : reject(
            new Error(`generated SDK exited ${code}: ${errors}\n${output}`),
          ),
    );
  });
  try {
    await Promise.race([
      Promise.all([finished, closed]),
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          child.kill();
          reject(new Error('generated socket SDK deadline exceeded'));
        }, 15000);
      }),
    ]);
    return output;
  } finally {
    clearTimeout(timer);
  }
}
