import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import {
  mkdtemp,
  mkdir,
  readFile,
  writeFile,
  symlink,
  rm,
} from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
import { createServer as createTcpServer } from 'node:net';
import { Duplex } from 'node:stream';
import { createHash } from 'node:crypto';

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
);
const compileOnly = process.argv.includes('--compile-only');
const frameworkRoot = path.resolve(
  root,
  process.env.FORKLAUNCH_TEST_FRAMEWORK_ROOT ?? 'framework-rs',
);
const temporary = await mkdtemp(path.join(tmpdir(), 'fl-ws-typescript-sdk-'));
const require = createRequire(import.meta.url);
const external = createRequire(
  path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES, 'package.json'),
);
const parser = compileOnly
  ? external
  : createRequire(
      require.resolve(process.env.ASYNCAPI_PARSER_PATH ?? '@asyncapi/parser'),
    );
const sdkDependencies = process.env.FORKLAUNCH_TEST_WS_SDK_DEPENDENCIES
  ? createRequire(
      path.join(
        process.env.FORKLAUNCH_TEST_WS_SDK_DEPENDENCIES,
        'package.json',
      ),
    )
  : parser;
const target = path.resolve(
  root,
  process.env.CARGO_TARGET_DIR ?? 'framework-rs/target',
);
const clients = new Set(),
  sockets = new Set();
let server, child, timer;
try {
  await mkdir(path.join(temporary, 'src'));
  await mkdir(path.join(temporary, 'node_modules/@types'), { recursive: true });
  for (const [name, resolver] of [
    ['ws', external],
    ['@types/ws', external],
    ['@types/node', external],
    ['ajv', sdkDependencies],
    ['ajv-formats', sdkDependencies],
    ['ajv-draft-04', sdkDependencies],
  ]) {
    await symlink(
      path.dirname(resolver.resolve(`${name}/package.json`)),
      path.join(temporary, 'node_modules', name),
      'dir',
    );
  }
  await writeFile(
    path.join(temporary, 'package.json'),
    JSON.stringify({ private: true, type: 'module' }),
  );
  await writeFile(
    path.join(temporary, 'tsconfig.json'),
    JSON.stringify({
      compilerOptions: {
        strict: true,
        target: 'ES2022',
        module: 'NodeNext',
        moduleResolution: 'NodeNext',
        esModuleInterop: true,
        types: ['node'],
        skipLibCheck: false,
        rootDir: 'src',
        outDir: 'compiled',
      },
      include: ['src/*.ts'],
    }),
  );
  execFileSync(
    'cargo',
    [
      'test',
      '--quiet',
      '--manifest-path',
      path.join(frameworkRoot, 'Cargo.toml'),
      '-p',
      'forklaunch-core','--features','build,asyncapi-import',
      'generated_typescript_socket_client_fixture',
    ],
    {
      cwd: root,
      env: {
        ...process.env,
        CARGO_INCREMENTAL: '0',
        FORKLAUNCH_TEST_TS_WS_SDK_OUTPUT: path.join(
          temporary,
          'src/generated.ts',
        ),
      },
      stdio: 'inherit',
      timeout: 300000,
    },
  );
  await writeFile(
    path.join(temporary, 'src/generated.ts'),
    `${await readFile(path.join(temporary, 'src/generated.ts'), 'utf8')}\nexport { SocketCodec };\n`,
  );
  await writeFile(
    path.join(temporary, 'src/native.ts'),
    await readFile(path.join(temporary, 'src/generated.ts.native')),
  );
  for (const suffix of ['recursive', 'empty', 'draft6', 'draft4', 'dictionary'])
    await writeFile(
      path.join(temporary, `src/${suffix}.ts`),
      (await readFile(path.join(temporary, `src/generated.ts.${suffix}`), 'utf8')) +
        (suffix === 'dictionary' ? '\nexport { generatedSocketSchemas };\n' : ''),
    );
  await writeFile(
    path.join(temporary, 'src/typed-usage.ts'),
    `
import { ForkLaunchSocketClient } from './generated.js';
import type { SocketIncoming as Dictionary } from './dictionary.js';
export const dictionary: Dictionary = { item: { '$ref': '#/definitions/Value' }, const: 'ok' };
// @ts-expect-error Referenced property types survive contextual bundling.
export const invalidDictionary: Dictionary = { item: { '$ref': '#/definitions/Value' }, const: 1 };
import type { SocketIncoming as Tree } from './recursive.js';
import type { SocketIncoming as DraftFour } from './draft4.js';
export const draftFour: DraftFour = 'not the ignored const';
// @ts-expect-error Draft 4 retains its declared string type.
export const invalidDraftFour: DraftFour = 42;
import type { SocketIncoming as DraftSix } from './draft6.js';
export const draftSix: DraftSix = [1, 2];
// @ts-expect-error Draft 6 arrays retain their numeric item type.
export const invalidDraftSix: DraftSix = ['wrong'];
export const tree: Tree = { value: 1, next: { value: 2 } };
// @ts-expect-error Recursive nodes retain their field types.
export const invalidTree: Tree = { value: 'wrong' };
export async function types(client: ForkLaunchSocketClient) {
  await client.send({ message: 'hello' });
  // @ts-expect-error Messages have their declared field types.
  await client.send({ message: 42 });
  // @ts-expect-error Controls have independent declared fields.
  await client.ping({ message: 'wrong' });
  const event = await client.recv();
  if (event?.type === 'message') { const text: string = event.value.message; void text; }
  if (event?.type === 'pong') { const timestamp: number = event.value.ts; void timestamp; }
  if (event?.type === 'close') { const reason: string = event.reason.reason; void reason; }
}
`,
  );
  execFileSync(
    process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc'),
    ['--project', path.join(temporary, 'tsconfig.json')],
    { cwd: root, stdio: 'inherit', timeout: 300000 },
  );
  await writeFile(
    path.join(temporary, 'tsconfig.cjs.json'),
    JSON.stringify({
      extends: './tsconfig.json',
      compilerOptions: {
        module: 'CommonJS',
        moduleResolution: 'Node',
        outDir: 'compiled-cjs',
        declaration: true,
      },
    }),
  );
  execFileSync(
    process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc'),
    ['--project', path.join(temporary, 'tsconfig.cjs.json')],
    { cwd: root, stdio: 'inherit', timeout: 300000 },
  );
  await writeFile(
    path.join(temporary, 'compiled-cjs/package.json'),
    JSON.stringify({ type: 'commonjs' }),
  );
  const commonjs = require(path.join(temporary, 'compiled-cjs/generated.js'));
  assert.equal(commonjs.SOCKET_PATH, '/chat');
  assert.doesNotThrow(() =>
    new commonjs.SocketCodec({ type: 'string' }, 1024).encode('hello'),
  );
  const { ForkLaunchSocketClient: Client, SocketCodec } = await import(
    pathToFileURL(path.join(temporary, 'compiled/generated.js'))
  );
  const { ForkLaunchSocketClient: NativeClient } = await import(
    pathToFileURL(path.join(temporary, 'compiled/native.js'))
  );
  for (const Codec of [SocketCodec, commonjs.SocketCodec]) {
    for (const version of ['draft-04', 'draft-06']) {
      const identity = {
        id: 'https://example.invalid/ignored',
        $id: 'https://example.invalid/ignored',
      };
      const schema = {
        $schema: `http://json-schema.org/${version}/schema#`,
        type: 'string',
        if: { ...identity },
        then: { ...identity },
      };
      const original = structuredClone(schema);
      const ignored = new Codec(schema, 4096);
      assert.equal(ignored.decode(ignored.encode('value')), 'value');
      assert.deepEqual(
        schema,
        original,
        'validation must not mutate caller schemas',
      );
      const selected = new Codec(
        {
          $schema: schema.$schema,
          $ref: '#/if/properties/id',
          if: { properties: { id: { type: 'string', minLength: 2 } } },
          then: { ...identity },
          else: { ...identity },
        },
        4096,
      );
      assert.equal(selected.decode(selected.encode('valid')), 'valid');
      assert.throws(() => selected.encode('x'));
      const recursive = new Codec(
        {
          $schema: schema.$schema,
          $ref: '#/if',
          if: {
            type: 'object',
            required: ['value'],
            properties: { value: { type: 'integer' }, next: { $ref: '#/if' } },
          },
          then: { ...identity },
          else: { ...identity },
        },
        4096,
      );
      const tree = { value: 1, next: { value: 2 } };
      assert.deepEqual(recursive.decode(recursive.encode(tree)), tree);
      assert.throws(() =>
        recursive.encode({ value: 1, next: { value: 'wrong' } }),
      );
    }
    for (const [dialect, keyword] of [
      [undefined, 'prefixItems'],
      ['http://json-schema.org/draft-07/schema#', 'dependentSchemas'],
      ['http://json-schema.org/draft-07/schema#', 'contentSchema'],
      ['https://json-schema.org/draft/2019-09/schema', 'prefixItems'],
      ['https://json-schema.org/draft/2020-12/schema', 'additionalItems'],
    ]) {
      const schema = {
        ...(dialect ? {$schema: dialect} : {}), type: 'string',
        [keyword]: {
          first: {$id: 'https://example.invalid/annotation', $ref: 'missing'},
          second: {$id: 'https://example.invalid/annotation'},
        },
      };
      const original = structuredClone(schema);
      const codec = new Codec(schema, 4096);
      assert.equal(codec.decode(codec.encode('value')), 'value');
      assert.throws(() => codec.encode(42));
      assert.deepEqual(schema, original);
      const selected = new Codec({
        ...(dialect ? {$schema: dialect} : {}),
        $ref: '#/' + keyword + '/first',
        [keyword]: {first: {type: 'string', minLength: 2}},
      }, 4096);
      assert.equal(selected.decode(selected.encode('valid')), 'valid');
      assert.throws(() => selected.encode('x'));
    }
    const literal = {
      $id: 'literal',
      id: 'literal',
      nested: { $anchor: 'literal' },
    };
    const selectedLiteral = new Codec(
      {
        $schema: 'http://json-schema.org/draft-06/schema#',
        $ref: '#/if',
        if: { const: literal },
        then: { $id: 'https://example.invalid/ignored' },
        else: { $id: 'https://example.invalid/ignored' },
      },
      4096,
    );
    assert.deepEqual(
      selectedLiteral.decode(selectedLiteral.encode(literal)),
      literal,
    );
    assert.throws(() =>
      selectedLiteral.encode({ ...literal, id: 'different' }),
    );
  }
  const nestedPrototype = JSON.parse(
    '{"type":"object","required":["水/outer~"],"properties":{"水/outer~":{"type":"object","required":["__proto__"],"properties":{"__proto__":{"$ref":"#/$defs/Allowed"}},"additionalProperties":false}},"$defs":{"Allowed":{"type":"integer","minimum":2}}}',
  );
  const nestedCodec = new SocketCodec(nestedPrototype, 4096);
  assert.deepEqual(
    nestedCodec.decode(Buffer.from('{"水/outer~":{"__proto__":3}}')),
    JSON.parse('{"水/outer~":{"__proto__":3}}'),
  );
  assert.throws(
    () => nestedCodec.decode(Buffer.from('{"水/outer~":{"__proto__":1}}')),
    /Invalid WebSocket payload/,
  );
  const scopedPrototype = JSON.parse(
    '{"$id":"https://schemas.example.test/root.json","type":"object","properties":{"outer":{"$id":"child.json","type":"object","required":["__proto__"],"properties":{"__proto__":{"$ref":"#/$defs/Allowed"}},"$defs":{"Allowed":{"type":"integer","minimum":2}},"additionalProperties":false}},"required":["outer"]}',
  );
  const scopedCodec = new SocketCodec(scopedPrototype, 4096);
  assert.doesNotThrow(() =>
    scopedCodec.decode(Buffer.from('{"outer":{"__proto__":3}}')),
  );
  assert.throws(
    () => scopedCodec.decode(Buffer.from('{"outer":{"__proto__":1}}')),
    /Invalid WebSocket payload/,
  );
  const exactPattern = String.raw`^__proto__(?![\s\S])`;
  const retainedPattern = JSON.parse(
    '{"type":"object","properties":{"__proto__":{"type":"integer"}},"additionalProperties":false}',
  );
  retainedPattern.properties.other = {
    $ref: `#/patternProperties/${encodeURIComponent(exactPattern)}`,
  };
  retainedPattern.patternProperties = { [exactPattern]: { maximum: 3 } };
  const retainedCodec = new SocketCodec(retainedPattern, 4096);
  assert.doesNotThrow(() =>
    retainedCodec.decode(
      Buffer.from('{"__proto__":2,"other":"unconstrained string"}'),
    ),
  );
  assert.throws(
    () => retainedCodec.decode(Buffer.from('{"__proto__":4}')),
    /Invalid WebSocket payload/,
  );
  const literal = JSON.parse('{"properties":{"__proto__":{"type":"string"}}}');
  const literalCodec = new SocketCodec({ const: literal }, 4096);
  assert.doesNotThrow(() => literalCodec.encode(literal));
  const { generatedSocketSchemas: dictionarySchemas } = await import(pathToFileURL(path.join(temporary, 'compiled/dictionary.js')));
  for (const Codec of [SocketCodec, commonjs.SocketCodec]) {
    const dictionaryCodec = new Codec(dictionarySchemas.incoming, 4096);
    const value = {item:{$ref:'#/definitions/Value'},const:'ok'};
    assert.deepEqual(dictionaryCodec.decode(dictionaryCodec.encode(value)),value);
    assert.equal(dictionaryCodec.decode(dictionaryCodec.encode(42)),42);
    assert.throws(()=>dictionaryCodec.encode({...value,const:1}));
    assert.throws(()=>dictionaryCodec.encode({...value,item:'wrong'}));
  }
  if (compileOnly) {
    console.log(
      'PASS generated TypeScript WebSocket consumers: strict ESM/CommonJS declarations, normal/recursive/empty/dictionary clients and prototype/reference-safe codecs',
    );
  } else {
    execFileSync(
      process.execPath,
      [path.join(root, 'framework-rs/parity/rust_ws_jsonschema_parity.mjs')],
      {
        cwd: root,
        env: {
          ...process.env,
          CARGO_INCREMENTAL: '0',
          FORKLAUNCH_TEST_TS_WS_SDK_CODEC_MODULE: path.join(
            temporary,
            'compiled/generated.js',
          ),
        },
        stdio: 'inherit',
        timeout: 300000,
      },
    );
    for (const options of [
      { handshakeTimeoutMs: 2 ** 31 },
      { writeTimeoutMs: 0 },
      { maxMessageBytes: 0 },
    ])
      await assert.rejects(
        Client.connect('ws://127.0.0.1:9/chat', options),
        /Invalid/,
      );
    const { WebSocketServer } = external('ws');
    const rawScenario = async (options, exercise) => {
      server = new WebSocketServer({ host: '127.0.0.1', port: 0 });
      await once(server, 'listening');
      const accepted = once(server, 'connection');
      const client = await Client.connect(
        `ws://127.0.0.1:${server.address().port}/chat`,
        options,
      );
      clients.add(client);
      const [socket] = await accepted;
      sockets.add(socket);
      const closed = once(socket, 'close');
      await exercise(client, socket, closed);
      client.terminate();
      if (socket.readyState !== 3) socket.terminate();
      await new Promise((resolve) => server.close(resolve));
      server = undefined;
    };
    for (const invalid of [
      { message: '' },
      { message: 'valid', extra: true },
      { message: 42 },
    ]) {
      await rawScenario({}, async (client, socket, closed) => {
        socket.send(JSON.stringify(invalid));
        await assert.rejects(
          client.recv({ timeoutMs: 2000 }),
          /Invalid WebSocket payload/,
        );
        assert.equal((await closed)[0], 1008);
      });
    }
    await rawScenario({}, async (client, socket, closed) => {
      socket.ping(JSON.stringify({ ts: 0 }));
      await assert.rejects(
        client.recv({ timeoutMs: 2000 }),
        /Invalid WebSocket payload/,
      );
      assert.equal((await closed)[0], 1008);
      assert.equal(await client.recv(), null);
    });
    await rawScenario(
      { maxQueuedEvents: 1 },
      async (client, socket, closed) => {
        socket.send(JSON.stringify({ message: 'first' }));
        socket.send(JSON.stringify({ message: 'second' }));
        assert.equal((await closed)[0], 1008);
        assert.deepEqual(await client.recv(), {
          type: 'message',
          value: { message: 'first' },
        });
        await assert.rejects(client.recv(), /queue limit/);
      },
    );
    await rawScenario({ maxQueuedBytes: 3 }, async (client, socket, closed) => {
      socket.send(JSON.stringify({ message: 'first' }));
      assert.equal((await closed)[0], 1008);
      await assert.rejects(client.recv(), /queue limit/);
    });
    await rawScenario({}, async (client, socket, closed) => {
      socket.send(JSON.stringify({ message: 'first' }));
      for await (const event of client) {
        assert.equal(event.type, 'message');
        break;
      }
      assert.equal((await closed)[0], 1006);
      assert.equal(await client.recv(), null);
    });
    await rawScenario({}, async (client, socket, closed) => {
      socket.close(1000, 'schema-invalid');
      await assert.rejects(
        client.recv({ timeoutMs: 2000 }),
        /Invalid WebSocket payload/,
      );
      await closed;
    });
    for (const cancelled of [false, true]) {
      const wire = stalledSocket();
      const client = await Client.connect('ws://127.0.0.1:9/chat', {
        webSocketOptions: { createConnection: () => wire },
        writeTimeoutMs: 25,
      });
      clients.add(client);
      const controller = new AbortController();
      const write = client.send(
        { message: 'stalled' },
        { signal: controller.signal },
      );
      await assert.rejects(client.ping({ ts: 42 }), /Only one WebSocket write/);
      if (cancelled) controller.abort();
      await assert.rejects(
        write,
        cancelled ? { name: 'AbortError' } : /write timed out/,
      );
      assert.equal(wire.destroyed, true);
      assert.equal(await client.recv(), null);
    }
    {
      const wire = stalledSocket();
      const client = await Client.connect('ws://127.0.0.1:9/chat', {
        webSocketOptions: { createConnection: () => wire },
        writeTimeoutMs: 25,
      });
      clients.add(client);
      const ping = Buffer.from(JSON.stringify({ ts: 42 }));
      wire.push(Buffer.concat([Buffer.from([0x89, ping.length]), ping]));
      assert.deepEqual(await client.recv({ timeoutMs: 2000 }), {
        type: 'ping',
        value: { ts: 42 },
      });
      await assert.rejects(
        client.recv({ timeoutMs: 2000 }),
        /pong write timed out/,
      );
      assert.equal(wire.destroyed, true);
    }
    {
      const wire = stalledSocket();
      const client = await Client.connect('ws://127.0.0.1:9/chat', {
        webSocketOptions: { createConnection: () => wire },
        closeTimeoutMs: 25,
      });
      clients.add(client);
      client.close(1000, { reason: 'done' });
      await assert.rejects(client.recv({ timeoutMs: 2000 }), /close timed out/);
      assert.equal(wire.destroyed, true);
      assert.equal(await client.recv(), null);
    }
    await rawScenario({}, async (client, socket, closed) => {
      socket.terminate();
      await assert.rejects(
        client.recv({ timeoutMs: 2000 }),
        /without a handshake/,
      );
      await closed;
      assert.equal(await client.recv(), null);
    });
    const heldSockets = new Set();
    const held = createTcpServer((socket) => {
      heldSockets.add(socket);
      socket.on('close', () => heldSockets.delete(socket));
    });
    await new Promise((resolve) => held.listen(0, '127.0.0.1', resolve));
    try {
      const url = `ws://127.0.0.1:${held.address().port}/chat`;
      await assert.rejects(
        Client.connect(url, { handshakeTimeoutMs: 25 }),
        /timed out/,
      );
      const controller = new AbortController();
      const connection = Client.connect(url, { signal: controller.signal });
      controller.abort();
      await assert.rejects(connection, { name: 'AbortError' });
    } finally {
      for (const socket of heldSockets) socket.destroy();
      await new Promise((resolve) => held.close(resolve));
    }
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
    let messages = 0;
    const closed = new Promise((resolve, reject) => {
      server.on('error', reject);
      server.on('connection', (socket) => {
        sockets.add(socket);
        socket.on('error', reject);
        socket.on('message', (message) => {
          messages++;
          socket.send(message);
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
    const url = `ws://127.0.0.1:${server.address().port}/chat`;
    await assert.rejects(Client.connect(url), /401/);
    const client = await Client.connect(url, {
      headers: { authorization: 'Bearer fixture-token' },
      protocols: ['chat.v1'],
    });
    clients.add(client);
    assert.equal(client.response.statusCode, 101);
    assert.equal(client.protocol, 'chat.v1');
    await cancelReceives(client);
    await assert.rejects(
      client.send({ message: '' }),
      /Invalid WebSocket payload/,
    );
    await assert.rejects(
      client.send({ message: 'hello', extra: true }),
      /Invalid WebSocket payload/,
    );
    await assert.rejects(client.ping({ ts: 0 }), /Invalid WebSocket payload/);
    await client.send({ message: 'hello 水 🦀' });
    assert.deepEqual(await client.recv({ timeoutMs: 2000 }), {
      type: 'message',
      value: { message: 'hello 水 🦀' },
    });
    await client.ping({ ts: 42 });
    assert.deepEqual(await client.recv({ timeoutMs: 2000 }), {
      type: 'pong',
      value: { ts: 42 },
    });
    client.close(1000, { reason: 'done' });
    assert.deepEqual(await client.recv({ timeoutMs: 2000 }), {
      type: 'close',
      code: 1000,
      reason: { reason: 'done' },
    });
    assert.equal(await client.recv(), null);
    await closed;
    assert.equal(messages, 1);
    await new Promise((resolve) => server.close(resolve));
    server = undefined;

    execFileSync(
      'cargo',
      [
        'build',
        '--quiet',
        '--manifest-path',
        path.join(frameworkRoot, 'Cargo.toml'),
        '-p',
        'forklaunch-ws',
        '--all-features',
        '--example',
        'websocket_runtime',
      ],
      {
        cwd: root,
        env: { ...process.env, CARGO_INCREMENTAL: '0' },
        stdio: 'inherit',
        timeout: 300000,
      },
    );
    child = spawn(
      path.join(target, 'debug/examples/websocket_runtime'),
      ['server'],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
    let output = '',
      errors = '';
    const nativeDone = new Promise((resolve, reject) => {
      child.once('error', reject);
      child.stdout.on('data', (data) => (output += data));
      child.stderr.on('data', (data) => (errors += data));
      child.once('exit', (code) =>
        code === 0
          ? resolve()
          : reject(
              new Error(`native peer exited ${code}: ${errors}\n${output}`),
            ),
      );
    });
    nativeDone.catch(() => {});
    timer = setTimeout(() => child.kill(), 15000);
    const nativeUrl = await new Promise((resolve, reject) => {
      const check = () => {
        const match = output.match(/ws:\/\/\S+/);
        if (match) {
          child.stdout.off('data', check);
          resolve(match[0]);
        }
      };
      child.stdout.on('data', check);
      nativeDone.then(
        () => reject(new Error('native peer ended before URL')),
        reject,
      );
      check();
    });
    await assert.rejects(NativeClient.connect(nativeUrl), /401/);
    const native = await NativeClient.connect(nativeUrl, {
      headers: { authorization: 'Bearer fixture-token' },
    });
    clients.add(native);
    await cancelReceives(native);
    await native.send({ message: 'hello 水' });
    assert.deepEqual(await native.recv({ timeoutMs: 2000 }), {
      type: 'message',
      value: { accepted: '水 🦀' },
    });
    assert.deepEqual(await native.recv({ timeoutMs: 2000 }), {
      type: 'ping',
      value: { ts: 42 },
    });
    await native.pong({ ts: 43 });
    assert.deepEqual(await native.recv({ timeoutMs: 2000 }), {
      type: 'close',
      code: 1000,
      reason: { reason: 'done' },
    });
    assert.equal(await native.recv(), null);
    await nativeDone;
    clearTimeout(timer);
    assert.match(output, /PASS native server/);
    console.log(
      'PASS generated TypeScript WebSocket SDK: strict ESM/types, actual TypeScript and Rust peers, authorization/subprotocols, original-field validation, Unicode, typed controls/close, receive cancellation, 1008 malformed-peer rejection, bounded queues, iterator cleanup, handshake cancellation and partial-write timeout/cancellation',
    );
  }
} finally {
  clearTimeout(timer);
  for (const client of clients) client.terminate();
  for (const socket of sockets) if (socket.readyState !== 3) socket.terminate();
  if (server) await new Promise((resolve) => server.close(resolve));
  if (child && child.exitCode === null) child.kill();
  await rm(temporary, { recursive: true, force: true });
}
async function cancelReceives(client) {
  await assert.rejects(client.recv({ timeoutMs: 2 ** 31 }), /Invalid read timeout: exceeds timer range/);
  assert.throws(() => client.close(1005), /Reserved/);
  await assert.rejects(client.recv({ timeoutMs: 5 }), /read timed out/);
  const controller = new AbortController();
  const pending = client.recv({ signal: controller.signal });
  await assert.rejects(client.recv(), /Only one/);
  controller.abort();
  await assert.rejects(pending, { name: 'AbortError' });
}
function stalledSocket() {
  let upgraded = false;
  return new Duplex({
    read() {},
    write(chunk, encoding, callback) {
      if (upgraded) return; // A partial frame write remains pending until termination.
      const key = chunk.toString().match(/Sec-WebSocket-Key: ([^\r\n]+)/i)?.[1];
      assert.ok(key);
      upgraded = true;
      const accept = createHash('sha1')
        .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
        .digest('base64');
      callback();
      queueMicrotask(() =>
        this.push(
          Buffer.from(
            `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
          ),
        ),
      );
    },
  });
}
