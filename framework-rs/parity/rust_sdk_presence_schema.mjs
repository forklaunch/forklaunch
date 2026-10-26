import { actualValidator } from './rust_actual_validator_fixture.mjs';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const escapedValue = "line\n'\\\0\u2028\u2029";
export const escapedKey = "wire\t'\\\n\0";
export function presenceSchema() {
  const { statics: v, validator } = actualValidator(root);
  const nested = v.schemify({ required: v.string.nullable(), optional: v.optional(v.string), optionalNull: v.optional(v.string.nullable()) }).strict();
  const schema = v.schemify({ required: v.string.nullable(), optional: v.optional(v.string), optionalNull: v.optional(v.string.nullable()), nested: v.optional(nested.nullable()), list: v.array(nested.nullable()), choice: v.optional(v.union([v.string, nested, v.null_])), escaped: v.optional(v.literal(escapedValue)), [escapedKey]: v.optional(v.string) }).strict();
  return { schema, validator };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { schema, validator } = presenceSchema();
  const absent = Symbol('absent');
  const values = [];
  for (const required of [absent, null, 'required'])
    for (const optional of [absent, null, 'optional'])
      for (const optionalNull of [absent, null, 'nullable'])
        for (const nested of [absent, null, { required: null }]) {
          const value = { list: [] };
          for (const [key, item] of Object.entries({ required, optional, optionalNull, nested })) if (item !== absent) value[key] = item;
          values.push(value);
        }
  for (const extra of [
    { nested: {} }, { nested: { required: null, optional: null } },
    { nested: { required: null, optionalNull: null } },
    { list: [null, { required: null, optionalNull: null }] }, { list: [{}] },
    { choice: null }, { choice: 'value' }, { choice: { required: null, optionalNull: null } }, { choice: {} },
    { extra: 'unexpected' }, { required: 42 }, { list: null },
  ]) values.push({ required: null, list: [], ...extra });
  values.push({ required: null });
  values.push({ required: null, list: [], escaped: escapedValue, [escapedKey]: 'hello 水' });
  const cases = values.map(value => {
    const result = validator.parse(schema, value);
    return { value, valid: result.ok, ...(result.ok ? { normalized: result.value } : {}) };
  });
  const app = process.argv[2];
  const directory = process.argv[3] ?? path.join(app, 'crates/presence/src');
  writeFileSync(path.join(directory, 'presence_schema.json'), JSON.stringify(validator.openapi(schema)));
  writeFileSync(path.join(directory, 'presence_cases.json'), JSON.stringify(cases));
  console.log(`Prepared ${cases.length} actual TypeScript validator presence/null cases`);
}
