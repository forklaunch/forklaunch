// Actual TS mapper wrappers + real Zod; the formatter-only import is isolated.
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const assert = require('node:assert/strict');
const deps = process.env.FORKLAUNCH_SDK_NODE_MODULES;
if (!deps) throw new Error('FORKLAUNCH_SDK_NODE_MODULES is required');
const ts = require(path.join(deps, 'typescript'));
const {z} = require(path.join(deps, 'zod'));
const source = path.resolve(__dirname, '../../../../framework/core/src/mappers/mapper.ts');
const loaded = new Module(source);
loaded.require = name => {
  if (name === '@forklaunch/validator') return {prettyPrintParseErrors: () => 'invalid DTO'};
  throw new Error(`Unexpected runtime dependency ${name}`);
};
loaded._compile(ts.transpileModule(fs.readFileSync(source, 'utf8'), {
  compilerOptions: {module: ts.ModuleKind.CommonJS},
}).outputText, source);
const schema = z.object({name: z.string().min(2).max(8), revision: z.number().int().min(1).max(100)});
const schemaValidator = {schemify: value => value, parse: (schema, value) => {
  const result = schema.safeParse(value);
  return result.success ? {ok: true, value: result.data} : {ok: false, errors: result.error.issues};
}};
(async () => {
  let called = false;
  const request = loaded.exports.requestMapper({schemaValidator, schema, entity: {}, mapperDefinition: {
    toEntity: async dto => { called = true; return dto.name; },
  }});
  await assert.rejects(request.toEntity({name: '', revision: 1}));
  assert.equal(called, false);
  assert.equal(await request.toEntity({name: 'valid', revision: 1}), 'valid');
  const response = loaded.exports.responseMapper({schemaValidator, schema, entity: {}, mapperDefinition: {
    toDto: async dto => dto,
  }});
  await assert.rejects(response.toDto({name: 'valid', revision: 101}));
  await assert.rejects(response.toDto({name: '', revision: 1}));
  assert.deepEqual(await response.toDto({name: 'valid', revision: 1}), {name: 'valid', revision: 1});
  console.log('PASS actual TS mapper request guard, response guard and successful values (real Zod)');
})().catch(error => { console.error(error); process.exitCode = 1; });
