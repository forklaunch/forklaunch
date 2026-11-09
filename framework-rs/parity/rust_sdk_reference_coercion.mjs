import assert from 'node:assert/strict';
import http from 'node:http';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { actualUniversalSdk } from './fixtures/rust_better_auth_frontend_client.mjs';
const { universalSdk, ResponseSchemas, schemaPointer, coerceReferenced } =
  await actualUniversalSdk();
const { default: Ajv } = await import(
  pathToFileURL(
    path.join(
      process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,
      'node_modules/ajv/dist/ajv.js'
    )
  )
);
const date = { type: 'string', format: 'date-time' },
  binary = { type: 'string', format: 'binary' };
const instant = '2026-10-14T00:00:00Z';
let payload,
  requests = 0;
let streaming = false;
const server = http.createServer((_, res) => {
  requests++;
  if (streaming) {
    res.setHeader('content-type', 'text/event-stream');
    res.end(
      `data: ${JSON.stringify(payload)}\n\ndata: ${JSON.stringify(payload)}\n\n`
    );
    return;
  }
  res.setHeader('content-type', 'application/json');
  res.end(JSON.stringify(payload));
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const host = `http://127.0.0.1:${server.address().port}`;
const doc = (schema, components = {}) => ({
  openapi: '3.1.0',
  info: { title: 'coercion', version: '1' },
  components: { schemas: components },
  paths: {
    '/value': {
      get: {
        operationId: 'value',
        responses: { 200: { content: { 'application/json': { schema } } } }
      }
    }
  }
});
const ref = (name) => ({ $ref: `#/components/schemas/${name}` });
async function call(document, value) {
  payload = value;
  const before = JSON.stringify(document);
  const sdk = await universalSdk({ host, registryOptions: { raw: document } });
  const response = (await sdk.value()).response;
  assert.equal(JSON.stringify(document), before, 'Registry is not mutated');
  return response;
}
try {
  const shape = { type: 'object', properties: { at: date, bytes: binary } };
  for (const schema of [shape, ref('Record')]) {
    const value = await call(doc(schema, { Record: shape }), {
      at: instant,
      bytes: 'AQI='
    });
    assert.equal(value.at.toISOString(), instant.replace('Z', '.000Z'));
    assert.deepEqual([...new Uint8Array(value.bytes)], [1, 2]);
  }
  assert.ok(
    (await call(doc(ref('Date'), { Date: date }), instant)) instanceof Date
  );
  assert.deepEqual(
    [
      ...new Uint8Array(
        await call(doc(ref('Binary'), { Binary: binary }), 'AQI=')
      )
    ],
    [1, 2]
  );
  const arrays = await call(
    doc(ref('Array'), {
      Array: { type: 'array', items: { type: 'array', items: ref('Date') } },
      Date: date
    }),
    [[instant], []]
  );
  assert.ok(arrays[0][0] instanceof Date);
  assert.deepEqual(arrays[1], []);
  const node = {
    type: ['object', 'null'],
    properties: { at: ref('Date'), next: ref('Node') }
  };
  const tree = await call(doc(ref('Node'), { Node: node, Date: date }), {
    at: instant,
    next: { at: instant, next: null }
  });
  assert.ok(tree.at instanceof Date && tree.next.at instanceof Date);
  assert.equal(tree.next.next, null);
  assert.ok(
    (await call(
      doc(ref('Date~1Time~0One'), { 'Date/Time~One': date }),
      instant
    )) instanceof Date
  );
  const resource = {
    $id: 'https://schemas.fixture.test/container',
    type: 'object',
    properties: { at: { $ref: 'date' } },
    $defs: { Date: { $id: 'date', ...date } }
  };
  assert.ok(
    (await call(doc(ref('Resource'), { Resource: resource }), { at: instant }))
      .at instanceof Date
  );
  const relative = {
    $id: 'https://schemas.fixture.test/root/',
    type: 'object',
    properties: {
      nested: {
        $id: 'part/',
        type: 'object',
        properties: { at: { $ref: 'date' } },
        $defs: { Date: { $id: 'date', ...date } }
      }
    }
  };
  assert.ok(
    (
      await call(
        doc(ref('Relative/properties/nested'), { Relative: relative }),
        { at: instant }
      )
    ).at instanceof Date
  );
  const inlineUnion = {
    type: 'object',
    properties: { at: { anyOf: [date, { type: 'string' }] } }
  };
  assert.ok(
    (await call(doc(inlineUnion), { at: instant })).at instanceof Date,
    'Legacy inline union policy remains unchanged'
  );
  const streamDoc = doc(ref('Envelope'), {
    Envelope: { type: 'object', properties: { data: ref('Record') } },
    Record: { type: 'object', properties: { at: date } }
  });
  const content = streamDoc.paths['/value'].get.responses[200].content;
  content['text/event-stream'] = content['application/json'];
  delete content['application/json'];
  const replacement = structuredClone(streamDoc);
  replacement.components.schemas.Record.properties.at = binary;
  streaming = true;
  payload = { at: instant };
  const streamClient = await universalSdk({
    host,
    registryOptions: { raw: streamDoc }
  });
  const iterator = (await streamClient.value()).response;
  assert.ok((await iterator.next()).value.data.at instanceof Date);
  streamClient.registryOpenApiJson = { latest: replacement };
  assert.ok(
    (await iterator.next()).value.data.at instanceof Date,
    'Active stream retains original document for coercion'
  );
  assert.equal((await iterator.next()).done, true);
  streaming = false;
  const dateDocument = doc(ref('Resource'), {
    Resource: { $id: 'https://schemas.fixture.test/shared', ...date }
  });
  const binaryDocument = doc(ref('Resource'), {
    Resource: { $id: 'https://schemas.fixture.test/shared', ...binary }
  });
  const versions = await universalSdk({
    host,
    registryOptions: { raw: { latest: binaryDocument, v1: dateDocument } }
  });
  payload = 'AQI=';
  assert.ok((await versions.value()).response instanceof ArrayBuffer);
  payload = instant;
  assert.ok((await versions.value['1']()).response instanceof Date);
  for (const union of [
    { anyOf: [date, { type: 'string' }] },
    { oneOf: [date, { type: 'null' }] }
  ]) {
    const result = await call(doc(ref('Union'), { Union: union }), instant);
    assert.equal(result instanceof Date, 'oneOf' in union);
  }
  assert.equal(
    await call(
      doc(ref('Nullable'), {
        Nullable: { type: ['string', 'null'], format: 'date-time' }
      }),
      null
    ),
    null
  );
  await assert.rejects(
    call(doc(ref('Date'), { Date: date }), 'invalid-date'),
    /Response does not match/
  );
  await assert.rejects(
    call(doc(ref('Binary'), { Binary: binary }), '%%%'),
    /Invalid base64 string/
  );

  // Direct coercion proves budgets/atomicity without AJV rejecting the fixture first.
  const child = (cursor, schema) => ({ schema, base: cursor.base });
  const cyclic = { $ref: '#loop' };
  const resolver = { child, reference: (cursor) => cursor };
  assert.equal(
    coerceReferenced(instant, { schema: cyclic, base: host }, resolver),
    instant
  );
  const invalid = { type: 'object', properties: { at: date, bad: binary } };
  const original = { at: instant, bad: '%%%' };
  assert.throws(
    () => coerceReferenced(original, { schema: invalid, base: host }, resolver),
    /Invalid base64/
  );
  assert.deepEqual(original, { at: instant, bad: '%%%' });
  assert.equal(
    coerceReferenced('invalid-date', { schema: date, base: host }, resolver),
    'invalid-date'
  );
  let recursive = {},
    current = recursive;
  for (let i = 0; i < 66; i++) {
    current.next = {};
    current = current.next;
  }
  const recursiveSchema = { type: 'object', properties: {} };
  recursiveSchema.properties.next = recursiveSchema;
  assert.throws(
    () =>
      coerceReferenced(
        recursive,
        { schema: recursiveSchema, base: host },
        resolver
      ),
    /budget exceeded/
  );
  assert.throws(
    () =>
      coerceReferenced(
        Array(10001).fill(instant),
        { schema: { type: 'array', items: date }, base: host },
        resolver
      ),
    /budget exceeded/
  );
  const chain = {
    child,
    reference: (cursor) => ({
      schema: { $ref: String(Number(cursor.schema.$ref) + 1) },
      base: host
    })
  };
  assert.throws(
    () =>
      coerceReferenced(instant, { schema: { $ref: '1' }, base: host }, chain),
    /reference budget exceeded/
  );
  const scoped = new ResponseSchemas(new Ajv({ strict: false }));
  const cyclicDocument = doc(ref('Loop'), { Loop: ref('Loop') });
  assert.equal(
    scoped.coerce(
      cyclicDocument,
      schemaPointer('/value', 'get', 200, 'application/json'),
      ref('Loop'),
      instant
    ),
    instant
  );
  console.log(
    `PASS referenced coercion: ${requests} real HTTP requests, inline baseline, root objects/scalars/arrays, recursive finite objects, escaped pointers, nested IDs/version isolation, unions/null/errors and immutable budget failure`
  );
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}
