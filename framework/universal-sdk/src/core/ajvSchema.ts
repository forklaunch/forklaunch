import type { AnySchema } from 'ajv';

const maps = new Set(['properties', 'patternProperties', '$defs', 'definitions', 'dependentSchemas', 'dependencies']);
const singles = new Set(['additionalProperties', 'additionalItems', 'contains', 'propertyNames', 'not', 'if', 'then', 'else', 'unevaluatedProperties', 'unevaluatedItems', 'contentSchema']);
const lists = new Set(['allOf', 'anyOf', 'oneOf', 'prefixItems']);
const own = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);
const pointer = (path: string[]) => path.map(key => encodeURIComponent(key.replace(/~/g, '~0').replace(/\//g, '~1'))).join('/');

/** Private AJV compilation view. Literal annotations and public input stay intact.
 * Only schema positions and explicit local pointer targets are rewritten.
 * Repairs inside dual-use default/const/enum data are rejected: rewriting those
 * targets would alter validation equality or injected default values.
 */
export function ajvSchema(input: AnySchema, idKey: 'id' | '$id' = '$id', document = false, defaultDialect?: string, inspectSchema?: (schema: Record<string, unknown>) => void): AnySchema {
  if (!input || typeof input !== 'object') return input;
  const dialect = typeof input.$schema === 'string' ? input.$schema : defaultDialect;
  const draft4 = idKey === 'id' || dialect?.includes('/draft-04/');
  const draft6 = dialect?.includes('/draft-06/');
  const draft2019 = dialect?.includes('/draft/2019-09/');
  const draft2020 = dialect?.includes('/draft/2020-12/');
  const legacy = !draft2019 && !draft2020;
  const ignored = new Set([
    ...(draft4 ? ['contains', 'propertyNames'] : []),
    ...(draft4 || draft6 ? ['if', 'then', 'else'] : []),
    ...(legacy ? ['dependentSchemas', 'unevaluatedProperties', 'unevaluatedItems', 'contentSchema'] : []),
    ...(legacy || draft2019 ? ['prefixItems'] : []),
    ...(draft2020 ? ['additionalItems', 'dependencies'] : [])
  ]);
  let remaining = 10000;
  const clones = new Map<string, Record<string, unknown> | unknown[]>();
  const originalAt = (path: string[]): unknown => {
    let value: unknown = input;
    for (const key of path) {
      if (!value || typeof value !== 'object' || !own(value, key)) return undefined;
      value = (value as Record<string, unknown>)[key];
    }
    return value;
  };
  function copy(path: string[]): Record<string, unknown> | unknown[] {
    const token = JSON.stringify(path);
    const existing = clones.get(token);
    if (existing) return existing;
    if (path.length > 64 || --remaining < 0) throw new Error('AJV schema traversal budget exceeded');
    const value = originalAt(path);
    if (!value || typeof value !== 'object') throw new Error('Invalid AJV schema location');
    if (Object.keys(value).length > remaining) throw new Error('AJV schema traversal budget exceeded');
    const result: Record<string, unknown> | unknown[] = Array.isArray(value) ? [...value] : {};
    if (!Array.isArray(value)) for (const key of Object.keys(value)) {
      Object.defineProperty(result, key, {value:(value as Record<string, unknown>)[key], enumerable:true, writable:true, configurable:true});
    }
    clones.set(token, result);
    if (path.length) Object.defineProperty(copy(path.slice(0,-1)), path.at(-1)!, {value:result, enumerable:true, writable:true, configurable:true});
    return result;
  }
  const pending: Array<{path:string[]; resource:string[]}> = [];
  function enqueue(path: string[], resource: string[]) {
    if (pending.length >= 10000 || path.length > 64) throw new Error('AJV schema traversal budget exceeded');
    pending.push({path,resource});
  }
  if (!document) enqueue([],[]);
  else {
    // OpenAPI container fields are not themselves JSON Schemas.
    const roots = input as Record<string, unknown>;
    const walk = (value: unknown, path: string[], depth: number): void => {
      if (!value || typeof value !== 'object') return;
      if (depth > 64 || --remaining < 0) throw new Error('AJV schema traversal budget exceeded');
      if (Object.keys(value).length > remaining) throw new Error('AJV schema traversal budget exceeded');
      for (const [key, child] of Object.entries(value)) {
        if (key === 'schema') enqueue([...path,key],[]);
        else if (key === 'schemas' && path.length === 1 && path[0] === 'components' && child && typeof child === 'object') {
          for (const name of Object.keys(child)) enqueue([...path,key,name],[]);
        } else if (['paths','webhooks','pathItems','components','responses','requestBodies','parameters','headers','callbacks','requestBody','content','get','put','post','delete','options','head','patch','trace'].includes(key) || path.at(-1)==='paths' || path.at(-1)==='webhooks' || path.at(-1)==='pathItems' || path.at(-2)==='callbacks' || path.at(-1)==='content' || ['responses','requestBodies','parameters','headers','callbacks'].includes(path.at(-1) ?? '') || Array.isArray(value)) walk(child,[...path,key],depth+1);
      }
    };
    walk(roots,[],0);
  }
  const seen = new Set<string>();
  const literalRanges: string[][] = [];
  const repairs: string[][] = [];
  while (pending.length) {
    const {path,resource} = pending.pop()!;
    const token = JSON.stringify(path);
    if (seen.has(token)) continue;
    seen.add(token);
    if (--remaining < 0) throw new Error('AJV schema traversal budget exceeded');
    const original = originalAt(path);
    if (!original || typeof original !== 'object' || Array.isArray(original)) continue;
    const node = copy(path) as Record<string, unknown>;
    inspectSchema?.(node);
    const base = typeof node[idKey] === 'string' ? path : resource;
    for (const key of ['default', 'const', 'enum']) {
      if (own(original, key)) literalRanges.push([...path, key]);
    }
    const properties = node.properties;
    if (properties && typeof properties === 'object' && own(properties,'__proto__')) {
      repairs.push(path);
      const patterns = node.patternProperties && typeof node.patternProperties === 'object'
        ? copy([...path,'patternProperties']) as Record<string, unknown> : {};
      let pattern = '^__proto__(?![\\s\\S])';
      while (own(patterns,pattern)) pattern += '(?:)';
      Object.defineProperty(patterns,pattern,{value:{$ref:`#/${pointer([...path.slice(base.length),'properties','__proto__'])}`},enumerable:true,writable:true,configurable:true});
      node.patternProperties = patterns;
    }
    if (typeof node.$ref === 'string' && node.$ref.startsWith('#/')) {
      const fragment = decodeURIComponent(node.$ref.slice(2)).split('/').map(key=>key.replace(/~1/g,'/').replace(/~0/g,'~'));
      enqueue([...base,...fragment],base);
    }
    for (const [key,child] of Object.entries(original)) {
      if (ignored.has(key)) continue;
      if (maps.has(key) && child && typeof child === 'object' && !Array.isArray(child)) {
        for (const name of Object.keys(child)) enqueue([...path,key,name],base);
      } else if ((lists.has(key) || key==='items') && Array.isArray(child)) child.forEach((_,index)=>enqueue([...path,key,String(index)],base));
      else if (singles.has(key) || key==='items') enqueue([...path,key],base);
    }
  }
  for (const path of repairs) {
    for (const range of literalRanges) {
      if (--remaining < 0) throw new Error('AJV schema traversal budget exceeded');
      if (range.length <= path.length && range.every((key, index) => path[index] === key)) {
        throw new Error('AJV own-property repair cannot rewrite a schema inside literal default/const/enum data');
      }
    }
  }
  return (clones.get('[]') ?? input) as AnySchema;
}
