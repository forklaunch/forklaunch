// Exercise the actual generated and published portable validators with AJV.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(path.join(process.env.FORKLAUNCH_TEST_SOCKET_DEPENDENCIES ?? '/private/tmp/fl-binary-socket-dependencies', 'package.json'));
const ts = require('typescript');
const directory = path.join(root, 'framework-rs/crates/core/src/typescript');
const read = name => readFileSync(path.join(directory, name), 'utf8');
// Match typescript_schema_runtime_source: the transport runtime receives these
// helpers together, rather than a stand-in validator or an old monolithic file.
const generated = read('schema-runtime.ts') + read('date-codec.ts') +
  read('zod-checks.ts').replace("import type Ajv from 'ajv';", '') +
  read('zod-regex.ts').replace("import type Ajv from 'ajv';", '') +
  `\nconst socketDraft6MetaSchema = ${read('draft6-schema.json')};`;
const published = readFileSync(path.join(root, 'framework/universal-sdk/src/portableRuntime.ts'), 'utf8');
let count = 0;
let regressionCount = 0;
for (const [name, source] of [['generated', generated], ['published', published]]) {
  const compiled = ts.transpileModule(source + '\nexport { socketValidator, socketAjvSchema };', {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText;
  const module = { exports: {} };
  vm.runInThisContext(`(function(module,exports,require){${compiled}\n})`, { filename: `${name}-schema-runtime.cjs` })(module, module.exports, require);
  const { socketValidator, socketAjvSchema } = module.exports;
  for (const dialect of [
    'http://json-schema.org/draft-04/schema#',
    'http://json-schema.org/draft-06/schema#',
    'http://json-schema.org/draft-07/schema#',
    'https://json-schema.org/draft/2019-09/schema',
    'https://json-schema.org/draft/2020-12/schema'
  ]) {
    const idKey = dialect.includes('draft-04') ? 'id' : '$id';
    const identity = { version: 1, id: 'openapi-component:/components/schemas/Tree', name: 'Tree', projection: 'value' };
    const schema = {
      $schema: dialect, [idKey]: 'https://example.invalid/tree',
      type: 'object', required: ['value'], additionalProperties: false,
      'x-forklaunch-model': identity,
      properties: {
        value: { type: 'integer' },
        children: { type: 'array', items: { $ref: '#', 'x-forklaunch-model': { ...identity } } },
        other: { [idKey]: 'https://example.invalid/other', type: 'string' },
        alias: { $ref: 'https://example.invalid/other' }
      }
    };
    const before = JSON.stringify(schema);
    const validate = socketValidator(schema);
    assert.equal(validate({ value: 1, children: [{ value: 2 }], alias: 'ok' }), true, `${name} ${dialect}`);
    assert.equal(validate({ value: 1, children: [{ value: 'bad' }] }), false);
    assert.equal(validate({ value: 1, alias: 2 }), false);
    assert.equal(JSON.stringify(schema), before, 'authored model identity must be immutable');
    const prepared = socketAjvSchema(schema);
    assert.equal(prepared['x-forklaunch-model'].id, undefined);
    assert.equal(prepared.properties.other[idKey], 'https://example.invalid/other', 'real resource identity is preserved');

    // Annotation storage selected by an explicit pointer is a real schema.
    const referenced = {
      $schema: dialect, $ref: '#/x-forklaunch-model',
      'x-forklaunch-model': { [idKey]: 'https://example.invalid/stored', type: 'integer' }
    };
    const check = socketValidator(referenced);
    assert.equal(check(4), true);
    assert.equal(check('bad'), false);
    assert.equal(socketAjvSchema(referenced)['x-forklaunch-model'][idKey], 'https://example.invalid/stored');

    const nestedReference = {
      $schema: dialect, type: 'object', properties: {
        nested: { [idKey]: 'https://example.invalid/nested', type: 'object', properties: { value: { $ref: '#/x-forklaunch-model' } },
          'x-forklaunch-model': { [idKey]: 'https://example.invalid/stored-nested', type: 'integer' } },
        alias: { $ref: 'https://example.invalid/stored-nested' }
      }
    };
    const nestedCheck = socketValidator(nestedReference);
    assert.equal(nestedCheck({ nested: { value: 1 }, alias: 2 }), true);
    assert.equal(nestedCheck({ nested: { value: 'bad' }, alias: 2 }), false);
    assert.equal(nestedCheck({ nested: { value: 1 }, alias: 'bad' }), false);

    // Literal values are not annotation/schema storage, even if they contain
    // metadata-shaped objects and resource-looking keys.
    const literal = { 'x-forklaunch-model': identity, id: 'literal-value', $id: 'literal-modern-value' };
    const literals = { $schema: dialect, type: 'object', default: literal, ...(dialect.includes('draft-04') ? {} : { const: literal }), enum: [literal] };
    assert.deepEqual(socketAjvSchema(literals), literals);
    const literalCheck = socketValidator(literals);
    assert.equal(literalCheck(literal), true);
    assert.equal(literalCheck({ ...literal, id: 'changed' }), false);

    // A reference can select a literal value as a schema. Normalizing metadata
    // inside that value would change equality/default data, so reject the
    // ambiguous schema before changing the compiler's private copy.
    for (const keyword of ['default', 'const', 'enum']) {
      for (const encodedPointer of [false, true]) {
        const value = { type: 'object', 'x-forklaunch-model': { id: 'literal-identity', name: 'literal' } };
        const pointer = `/properties/settings/${keyword}${keyword === 'enum' ? '/0' : ''}`;
        const dual = {
          $schema: dialect,
          $ref: `#${encodedPointer ? encodeURIComponent(pointer) : pointer}`,
          properties: { settings: { [keyword]: keyword === 'enum' ? [value] : value } }
        };
        const snapshot = JSON.stringify(dual);
        assert.throws(() => socketValidator(dual), /literal default\/const\/enum/);
        assert.equal(JSON.stringify(dual), snapshot, 'rejected literal schema must remain authored exactly');
        regressionCount++;
      }
      const dialectAlias = dialect.startsWith('http:') ? dialect.replace('http:', 'https:') : dialect + '#';
      const value = { $schema: dialectAlias, type: 'object' };
      const dualDialect = {
        $schema: dialect,
        $ref: `#/${keyword}${keyword === 'enum' ? '/0' : ''}`,
        [keyword]: keyword === 'enum' ? [value] : value
      };
      const snapshot = JSON.stringify(dualDialect);
      assert.throws(() => socketValidator(dualDialect), /literal default\/const\/enum/);
      assert.equal(JSON.stringify(dualDialect), snapshot);
      regressionCount++;
    }
    const harmlessDual = { $schema: dialect, $ref: '#/default', default: { type: 'integer' } };
    assert.deepEqual(socketAjvSchema(harmlessDual), harmlessDual);
    const harmlessCheck = socketValidator(harmlessDual);
    assert.equal(harmlessCheck(3), true);
    assert.equal(harmlessCheck('bad'), false);
    regressionCount++;

    // Explicit unknown-key targets are schemas even when ordinary keyword
    // traversal would skip them. Own __proto__ properties must remain typed,
    // including recursive targets and separate resource identities.
    for (const standalone of [false, true]) {
      const stored = {
        ...(standalone ? { [idKey]: 'https://example.invalid/proto-storage' } : {}),
        type: 'object', additionalProperties: false,
        properties: JSON.parse('{"__proto__":{"type":"integer"}}')
      };
      stored.properties.child = { $ref: standalone ? '#' : '#/x-forklaunch-model' };
      const pointed = { $schema: dialect, $ref: '#/x-forklaunch-model', 'x-forklaunch-model': stored };
      const snapshot = JSON.stringify(pointed);
      const pointedCheck = socketValidator(pointed);
      assert.equal(pointedCheck(JSON.parse('{"__proto__":1,"child":{"__proto__":2}}')), true);
      assert.equal(pointedCheck(JSON.parse('{"__proto__":"bad"}')), false);
      assert.equal(pointedCheck(JSON.parse('{"__proto__":1,"child":{"__proto__":"bad"}}')), false);
      assert.equal(JSON.stringify(pointed), snapshot);
      regressionCount++;
    }

    if (dialect.includes('draft-04') || dialect.includes('draft-06') || dialect.includes('draft-07')) {
      // Fragment-only legacy IDs are anchors, not new JSON Pointer roots. Test
      // both the document root and an embedded resource, with real aliases so
      // erroneously erasing the selected schema's ID fails compilation too.
      for (const embedded of [false, true]) for (const anchor of ['#child', 'https://example.invalid/anchor-owner#child']) {
        const anchored = {
          [idKey]: 'https://example.invalid/anchor-owner', type: 'object',
          properties: {
            nested: { [idKey]: anchor, type: 'object', properties: { value: { $ref: '#/x-forklaunch-model' }, ...JSON.parse('{"__proto__":{"type":"integer"}}') } },
            alias: { $ref: 'https://example.invalid/anchor-target' }
          },
          'x-forklaunch-model': { [idKey]: 'https://example.invalid/anchor-target', type: 'integer' }
        };
        const anchoredSchema = embedded
          ? { $schema: dialect, type: 'object', properties: { owner: anchored } }
          : { $schema: dialect, ...anchored };
        const wrap = value => embedded ? { owner: value } : value;
        const anchoredCheck = socketValidator(anchoredSchema);
        assert.equal(anchoredCheck(wrap({ nested: { value: 1 }, alias: 2 })), true);
        assert.equal(anchoredCheck(wrap({ nested: { value: 'bad' }, alias: 2 })), false);
        assert.equal(anchoredCheck(wrap({ nested: { value: 1 }, alias: 'bad' })), false);
        assert.equal(anchoredCheck(wrap({ nested: JSON.parse('{"value":1,"__proto__":2}'), alias: 2 })), true);
        assert.equal(anchoredCheck(wrap({ nested: JSON.parse('{"value":1,"__proto__":"bad"}'), alias: 2 })), false);
        regressionCount++;
      }
    }
    count++;
  }
}
console.log(`PASS ${count} generated/published dialect cases and ${regressionCount} literal/fragment-ID/recursive-own-property regressions`);
