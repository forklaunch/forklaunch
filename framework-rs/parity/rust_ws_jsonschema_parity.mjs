import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
);
const { load } = actualValidator(root),
  helpers = load(path.join(root, 'framework/core/src/ws/webSocketLike.ts'));
const require = createRequire(import.meta.url),
  parserPath = require.resolve(
    process.env.ASYNCAPI_PARSER_PATH ?? '@asyncapi/parser',
  ),
  external = createRequire(parserPath);
const Ajv = external('ajv'),
  AjvDraft4 = external('ajv-draft-04'),
  Ajv2019 = external('ajv/dist/2019').default,
  Ajv2020 = external('ajv/dist/2020').default,
  addFormats = external('ajv-formats');
const cache = new Map();
// Compare decimal representations exactly, rather than accepting a nearby
// floating-point value through AJV's multipleOfPrecision tolerance.
function decimal(value) {
  const [coefficient, exponent = '0'] = String(value).toLowerCase().split('e');
  const [integer, fraction = ''] = coefficient.split('.');
  return [BigInt(integer + fraction), Number(exponent) - fraction.length];
}
function multipleOf(value, divisor) {
  let [numerator, scale] = decimal(value);
  let [denominator, divisorScale] = decimal(divisor);
  scale -= divisorScale;
  if (scale >= 0) numerator *= 10n ** BigInt(scale);
  else denominator *= 10n ** BigInt(-scale);
  return numerator % denominator === 0n;
}
const validator = {
  parse(schema, value) {
    const key = JSON.stringify(schema);
    let validate = cache.get(key);
    if (!validate) {
      const engine = new (
        schema.$schema?.includes('/draft-04/')
          ? AjvDraft4
          : schema.$schema?.includes('2020-12')
            ? Ajv2020
            : schema.$schema?.includes('2019-09')
              ? Ajv2019
              : Ajv
      )({
        strict: false,
        validateFormats: true,
        ownProperties: true,
      });
      if (schema.$schema?.includes('/draft-06/'))
        engine.addMetaSchema(
          external('ajv/dist/refs/json-schema-draft-06.json'),
        );
      if (schema.$schema?.includes('/draft-04/'))
        for (const keyword of [
          'const',
          'contains',
          'propertyNames',
          'if',
          'then',
          'else',
        ])
          engine.removeKeyword(keyword);
      if (schema.$schema?.includes('/draft-06/'))
        for (const keyword of ['if', 'then', 'else'])
          engine.removeKeyword(keyword);
      addFormats(engine);
      engine.removeKeyword('multipleOf');
      engine.addKeyword({
        keyword: 'multipleOf',
        type: 'number',
        schemaType: 'number',
        compile: (divisor) => (value) => multipleOf(value, divisor),
      });
      // Independently cover AJV's excluded __proto__ property through a pattern;
      // generated clients also support nested/scoped versions of this case.
      const checked =
        schema.properties && Object.hasOwn(schema.properties, '__proto__')
          ? {
              ...schema,
              patternProperties: {
                '^__proto__(?![\\s\\S])': schema.properties.__proto__,
              },
            }
          : schema;
      validate = engine.compile(checked);
      cache.set(key, validate);
    }
    // The actual helpers execute in a VM; AJV constant equality expects values
    // from its own realm. Preserve the helper output while validating a JSON copy.
    return validate(JSON.parse(JSON.stringify(value)))
      ? { ok: true, value }
      : {
          ok: false,
          errors: validate.errors.map((error) => ({
            path: [error.instancePath],
            message: error.message,
          })),
        };
  },
};
const schemas = [
  { const: { '$serde_json::private::Number': '123' } },
  // A literal `$serde_json::private::RawValue` key is upstream serde_json's
  // raw_value marker (fb1e03d21 dropped the vendored patch), so it is not compared.

  ...[undefined, 'draft-04', 'draft-06', '2019-09', '2020-12'].flatMap(
    (draft) => {
      const dialect = draft
        ? {
            $schema: draft.startsWith('draft-')
              ? `http://json-schema.org/${draft}/schema#`
              : `https://json-schema.org/draft/${draft}/schema`,
          }
        : {};
      return [
        // AJV splits the raw fragment before percent-decoding each token.
        // Include conflicting nested targets to detect accidental RFC decoding.
        {
          ...dialect,
          $ref: '#/definitions/Foo%2FBar',
          definitions: {
            'Foo/Bar': { type: 'string' },
            Foo: { Bar: { type: 'number' } },
          },
        },
        {
          ...dialect,
          $ref: '#/definitions%2FValue',
          definitions: { Value: { type: 'string' } },
        },
        {
          ...dialect,
          $ref: '#/definitions%2fValue',
          'definitions/Value': { type: 'number' },
          definitions: { Value: { type: 'string' } },
        },
        ...['Foo%252FBar', 'Foo%7E1Bar', '%E6%B0%B4%2F%7E0'].map(
          ($ref, index) => ({
            ...dialect,
            $ref: `#/definitions/${$ref}`,
            definitions: {
              [['Foo%2FBar', 'Foo/Bar', '水/~'][index]]: {
                type: 'string',
              },
            },
          }),
        ),
        {
          ...dialect,
          $ref: '#/definitions/Node%2FThing',
          definitions: {
            'Node/Thing': {
              type: 'object',
              required: ['value'],
              properties: {
                value: { type: 'integer' },
                next: { $ref: '#/definitions/Node%2FThing' },
              },
              additionalProperties: false,
            },
          },
        },
        {
          ...dialect,
          allOf: [{ $ref: '#/default/ref%2Fpath' }],
          default: { 'ref/path': { type: 'object' } },
          const: { $ref: 'literal' },
        },
        {
          ...dialect,
          $ref: '#/definitions/Foo%2FBar',
          definitions: { 'Foo/Bar': { type: 'string' } },
          minLength: 3,
          maxLength: 8,
        },
        {
          ...dialect,
          $ref: '#/default/ref%2Fpath',
          default: { 'ref/path': { type: 'object' } },
          const: { $ref: 'literal' },
        },
        {
          ...dialect,
          $ref: '#/default/%30',
          default: [{ type: 'string' }],
        },
        ...['%FF', '%GG', '%'].map((token) => ({
          ...dialect,
          $ref: `#/definitions/${token}`,
          definitions: { [token]: { type: 'string' } },
        })),
        {
          ...dialect,
          [draft === 'draft-04' ? 'id' : '$id']: 'https://example.test/root',
          $ref: 'inner#/definitions/Foo%2FBar',
          definitions: {
            Resource: {
              [draft === 'draft-04' ? 'id' : '$id']: 'inner',
              definitions: { 'Foo/Bar': { type: 'string' } },
            },
          },
        },
        {
          ...dialect,
          [draft === 'draft-04' ? 'id' : '$id']: 'https://example.test/root',
          $ref: '#/definitions/Resource%2FValue/properties/self',
          definitions: {
            'Resource/Value': {
              [draft === 'draft-04' ? 'id' : '$id']: 'inner',
              properties: {
                self: { $ref: '#/definitions/Leaf%2FValue' },
              },
              definitions: { 'Leaf/Value': { type: 'integer' } },
            },
          },
        },
      ];
    },
  ),
  ...['#%2Fdefinitions/Value', '#%2fdefinitions/Value'].map(($ref) => ({
    $ref,
    definitions: { Value: { type: 'string' } },
  })),
  ...['draft-04', 'draft-06'].map((version) => ({
    $schema: `http://json-schema.org/${version}/schema#`,
    type: 'string',
    if: 42,
    then: false,
    else: true,
    dependentSchemas: { annotation: 42 },
    prefixItems: ['literal'],
    unevaluatedProperties: false,
    unevaluatedItems: true,
    contentSchema: false,
    ...(version === 'draft-04'
      ? { contains: 42, propertyNames: ['literal'] }
      : {}),
  })),
  {
    $schema: 'http://json-schema.org/draft-06/schema#',
    type: 'object',
    properties: { value: { type: 'string' } },
    if: { required: ['value'] },
    then: { required: ['other'] },
  },

  {
    $schema: 'http://json-schema.org/draft-04/schema#',
    type: 'array',
    items: { type: 'integer' },
    contains: { minimum: 100 },
  },
  {
    $schema: 'http://json-schema.org/draft-04/schema#',
    type: 'object',
    properties: { value: { type: 'string' } },
    propertyNames: { pattern: '^a$' },
    if: { required: ['value'] },
    then: { required: ['other'] },
  },

  {
    $schema: 'http://json-schema.org/draft-04/schema#',
    type: 'number',
    minimum: 0,
    exclusiveMinimum: true,
    maximum: 10,
    exclusiveMaximum: true,
    multipleOf: 0.1,
  },
  {
    $schema: 'http://json-schema.org/draft-04/schema#',
    type: 'string',
    const: 'ignored',
    minLength: 1,
  },
  {
    $schema: 'http://json-schema.org/draft-04/schema#',
    type: 'object',
    required: ['value'],
    properties: { value: { type: 'string', const: 'ignored' } },
    dependencies: { value: ['other'] },
  },
  { $schema: 'http://json-schema.org/draft-04/schema#', type: 'integer' },

  {
    $schema: 'http://json-schema.org/draft-06/schema#',
    type: 'number',
    exclusiveMinimum: 0,
    maximum: 10,
    multipleOf: 0.1,
  },
  {
    $schema: 'http://json-schema.org/draft-06/schema#',
    type: 'array',
    items: { type: 'integer' },
    contains: { minimum: 2 },
    minItems: 1,
    maxItems: 3,
  },
  {
    $schema: 'http://json-schema.org/draft-06/schema#',
    type: 'object',
    propertyNames: { pattern: '^[ab]$' },
    dependencies: { a: ['b'] },
    additionalProperties: { type: 'integer' },
  },
  true,
  false,
  {
    $schema: 'https://json-schema.org/draft/2019-09/schema',
    type: 'object',
    dependentRequired: { a: ['b'] },
  },
  { type: 'object', required: ['toString'] },
  JSON.parse(
    '{"type":"object","required":["__proto__"],"properties":{"__proto__":{"type":"object"}},"additionalProperties":false}',
  ),
  { type: 'string', minLength: 1, maxLength: 8, pattern: '^[a-z 水]+$' },
  { type: 'number', minimum: 0, maximum: 10, multipleOf: 0.1 },
  { type: 'integer', minimum: 0, exclusiveMaximum: 10 },
  {
    type: 'object',
    required: ['message'],
    properties: { message: { type: 'string', minLength: 1, maxLength: 8 } },
    additionalProperties: false,
  },
  {
    anyOf: [
      {
        type: 'object',
        required: ['kind', 'message'],
        properties: {
          kind: { const: 'chat' },
          message: { type: 'string', minLength: 1 },
        },
        additionalProperties: false,
      },
      {
        type: 'object',
        required: ['kind', 'count'],
        properties: {
          kind: { const: 'count' },
          count: { type: 'integer', minimum: 0 },
        },
        additionalProperties: false,
      },
    ],
  },
  {
    type: 'array',
    items: { type: 'integer' },
    minItems: 1,
    maxItems: 2,
    uniqueItems: true,
  },
  {
    type: 'array',
    items: [{ type: 'string' }, { type: 'integer' }],
    additionalItems: false,
    minItems: 2,
  },
  {
    type: 'object',
    properties: { a: { type: 'number' }, b: { type: 'number' } },
    dependencies: { a: ['b'] },
    additionalProperties: false,
  },
  {
    type: 'object',
    required: ['mode', 'value'],
    properties: { mode: { type: 'string' }, value: { type: 'number' } },
    if: { properties: { mode: { const: 'strict' } } },
    then: { properties: { value: { minimum: 2 } } },
    else: { properties: { value: { minimum: 0 } } },
  },
  { oneOf: [{ type: 'string' }, {}] },
  { const: { $ref: 'literal' } },
  { enum: ['abc', 1, null, { message: 'hello' }] },
  {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    type: 'object',
    allOf: [{ properties: { message: { type: 'string' } } }],
    unevaluatedProperties: false,
  },
  {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    type: 'array',
    contains: { const: 1 },
    minContains: 1,
    maxContains: 1,
  },
  { type: 'string', format: 'email' },
  { type: 'string', format: 'uuid' },
  { type: 'string', format: 'date' },
  { type: 'string', format: 'date-time' },
  {
    $ref: '#/definitions/Node%2525~1~0%20%E6%B0%B4',
    definitions: {
      'Node%25/~ 水': {
        type: 'object',
        required: ['value'],
        properties: {
          value: { type: 'integer' },
          next: { $ref: '#/definitions/Node%2525~1~0%20%E6%B0%B4' },
        },
        additionalProperties: false,
      },
    },
  },
  {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    type: 'array',
    prefixItems: [{ type: 'string' }, { type: 'integer' }],
    items: false,
    minItems: 2,
  },
  {
    $id: 'https://schemas.example.test/root.json',
    definitions: { child: { $id: 'child.json', type: 'string', minLength: 1 } },
    $ref: 'child.json',
  },
];
const values = [
  { '$serde_json::private::Number': '123' },
  // A literal `$serde_json::private::RawValue` key is upstream serde_json's
  // raw_value marker (fb1e03d21 dropped the vendored patch), so it is not compared.
  { nested: [{ '$serde_json::private::Number': { literal: true } }] },

  '',
  'hello 水',
  'abc',
  '😀😀😀😀😀😀😀😀',
  '😀😀😀😀😀😀😀😀😀',
  'bad!',
  'true',
  '42',
  'null',
  0,
  1,
  -1,
  0.3,
  0.30000000000000004,
  3.14,
  10,
  1001,
  null,
  true,
  false,
  [],
  [1],
  [1, 1],
  [1, 2],
  [1, 2, 3],
  ['hello', 1],
  {},
  { message: 'hello' },
  { message: '' },
  { message: 'hello', extra: true },
  { kind: 'chat', message: 'hello' },
  { kind: 'chat', message: 42 },
  { kind: 'count', count: 3 },
  { mode: 'strict', value: 1 },
  { mode: 'loose', value: 1 },
  { a: 1, b: 2 },
  { a: 1 },
  { value: 1, next: { value: 2 } },
  { $ref: 'literal' },
  { toString: 'own property' },
  JSON.parse('{"__proto__":{"allowed":true}}'),
  'a@example.com',
  'not@email',
  'a..b@example.com',
  'a@-bad.example',
  'urn:uuid:123e4567-e89b-42d3-a456-426614174000',
  '123e4567-e89b-42d3-a456-426614174000',
  'invalid-uuid',
  '2024-02-29',
  '2023-02-29',
  '2024-01-01T12:00:00Z',
];
const cases = [];
for (const schema of schemas)
  for (const value of values) {
    cases.push({ schema, action: 'encode', value });
    cases.push({
      schema,
      action: 'decode',
      bytes: [...Buffer.from(JSON.stringify(value))],
    });
  }
for (const schema of schemas)
  for (const text of ['raw text', '{broken', 'hello 水', '1.0', '1e0', '-0.0'])
    cases.push({ schema, action: 'decode', bytes: [...Buffer.from(text)] });
const temporary = await mkdtemp(path.join(tmpdir(), 'fl-ws-jsonschema-'));
const generated = process.env.FORKLAUNCH_TEST_TS_WS_SDK_CODEC_MODULE
  ? await import(
      pathToFileURL(process.env.FORKLAUNCH_TEST_TS_WS_SDK_CODEC_MODULE)
    )
  : undefined;
const generatedCodecs = new Map();
try {
  const input = path.join(temporary, 'input.json'),
    output = path.join(temporary, 'output.json');
  await writeFile(input, JSON.stringify(cases));
  execFileSync(
    'cargo',
    [
      'test',
      '--quiet',
      '--manifest-path',
      path.join(root, 'framework-rs/Cargo.toml'),
      '-p',
      // The codec tests moved into forklaunch-core's ws module (fb1e03d21).
      'forklaunch-core',
      '--all-features',
      'json_schema_payload_conformance',
    ],
    {
      cwd: root,
      env: {
        ...process.env,
        FORKLAUNCH_TEST_WS_JSON_SCHEMA_INPUT: input,
        FORKLAUNCH_TEST_WS_JSON_SCHEMA_OUTPUT: output,
      },
      stdio: 'inherit',
      timeout: 300000,
    },
  );
  const native = JSON.parse(await readFile(output, 'utf8'));
  for (let index = 0; index < cases.length; index++) {
    const input = cases[index];
    if (generated) {
      const key = JSON.stringify(input.schema);
      let value, error;
      try {
        if (!generatedCodecs.has(key))
          generatedCodecs.set(
            key,
            new generated.SocketCodec(input.schema, 1024 * 1024),
          );
        const codec = generatedCodecs.get(key);
        value =
          input.action === 'encode'
            ? codec.encode(input.value)
            : codec.decode(Buffer.from(input.bytes));
      } catch (cause) {
        error = cause;
      }
      assert.equal(
        native[index].ok,
        !error,
        `generated codec acceptance ${index}: ${JSON.stringify(input)} ${error ?? ''}`,
      );
      if (!error) {
        if (input.action === 'decode')
          assert.deepEqual(
            native[index].value,
            value,
            `generated decoded value ${index}`,
          );
        else if (typeof input.value === 'object' && input.value !== null)
          assert.deepEqual(
            JSON.parse(Buffer.from(native[index].bytes).toString()),
            JSON.parse(value.toString()),
            `generated structured bytes ${index}`,
          );
        else
          assert.deepEqual(
            Buffer.from(native[index].bytes),
            value,
            `generated wire bytes ${index}`,
          );
      }
    }
    let value, error;
    try {
      value =
        input.action === 'encode'
          ? helpers.normalizeEncodedValue(
              helpers.encodeSchemaValue(
                validator,
                input.value,
                input.schema,
                'fixture',
              ),
              'fixture',
            )
          : helpers.decodeSchemaValue(
              validator,
              Buffer.from(input.bytes),
              input.schema,
              'fixture',
            );
    } catch (cause) {
      error = cause;
    }
    assert.equal(
      native[index].ok,
      !error,
      `acceptance ${index}: ${JSON.stringify(input)} ${error ?? ''}`,
    );
    if (error) continue;
    if (input.action === 'decode')
      assert.deepEqual(
        JSON.parse(JSON.stringify(native[index].value)),
        JSON.parse(JSON.stringify(value)),
        `decoded value ${index}`,
      );
    else if (typeof input.value === 'object' && input.value !== null)
      assert.deepEqual(
        JSON.parse(Buffer.from(native[index].bytes).toString()),
        JSON.parse(value.toString()),
        `structured wire ${index}`,
      );
    else
      assert.deepEqual(
        Buffer.from(native[index].bytes),
        value,
        `wire bytes ${index}`,
      );
  }
  if (generated)
    console.log(
      `PASS ${cases.length} generated TypeScript/native JSON Schema codec comparisons`,
    );
  console.log(
    `PASS ${cases.length} actual TypeScript WebSocket helper/AJV and native JSON Schema codec cases: bounds, patterns, enums/unions, decimals, arrays/tuples, dependent/conditional properties, formats, recursive and scoped resource references, both directions and raw text`,
  );
} finally {
  await rm(temporary, { recursive: true, force: true });
}
