import { merge as deepMerge } from 'ts-deepmerge';

type Schema = Record<string, unknown>;
const maps = new Set(['properties', 'patternProperties', '$defs', 'definitions', 'dependentSchemas']);
const singles = new Set(['additionalProperties', 'additionalItems', 'items', 'contains', 'propertyNames', 'not', 'if', 'then', 'else', 'unevaluatedProperties', 'unevaluatedItems']);
const lists = new Set(['allOf', 'anyOf', 'oneOf', 'prefixItems']);

/** Preserve schema property names without restoring unsafe keys in arbitrary data.
 * Limits apply to schema positions, not literal default/const/enum payloads.
 */
export function merge(...schemas: object[]): Schema {
  const names = new Map<string, string>();
  const originals = new Map<string, string>();
  const active = new Set<object>();
  let remaining = 100000;
  function visit(value: unknown, restore: boolean, depth: number): unknown {
    if (depth > 64 || --remaining < 0) throw new Error('OpenAPI schema merge exceeds depth/node bounds');
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return value;
    if (active.has(value)) {
      throw new Error('OpenAPI schema merge exceeds depth/node bounds or contains a schema cycle');
    }
    active.add(value);
    try {
      const output: Schema = restore ? {} : Object.create(null);
      for (const [key, entry] of Object.entries(value)) {
        let projected = entry;
        if (maps.has(key) && entry && typeof entry === 'object' && !Array.isArray(entry)) {
          const map: Schema = restore ? {} : Object.create(null);
          for (const [name, child] of Object.entries(entry)) {
            if (--remaining < 0) throw new Error('OpenAPI schema merge exceeds node bounds');
            let mapped = name;
            if (key === 'properties') {
              if (restore) mapped = originals.get(name) ?? name;
              else {
                mapped = names.get(name) ?? `forklaunchProperty${names.size}`;
                names.set(name, mapped);
                originals.set(mapped, name);
              }
            }
            Object.defineProperty(map, mapped, {value:visit(child,restore,depth+1), enumerable:true, writable:true, configurable:true});
          }
          projected = map;
        } else if ((lists.has(key) || key === 'items') && Array.isArray(entry)) {
          if (entry.length > remaining) throw new Error('OpenAPI schema merge exceeds node bounds');
          projected = entry.map(child => visit(child,restore,depth+1));
        } else if (singles.has(key)) projected = visit(entry,restore,depth+1);
        Object.defineProperty(output,key,{value:projected,enumerable:true,writable:true,configurable:true});
      }
      return output;
    } finally { active.delete(value); }
  }
  const encoded = schemas.map(schema => visit(schema,false,0) as Schema);
  const combined = deepMerge(...encoded);
  remaining = 100000;
  return visit(combined,true,0) as Schema;
}
