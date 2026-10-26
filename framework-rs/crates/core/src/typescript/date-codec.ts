// Explicit portable date metadata; ordinary date-looking strings stay strings.
function portableDates(schema: AnySchema, value: unknown, encode = false, root: AnySchema = schema, depth = 0): unknown {
  if (depth === 0 && !JSON.stringify(schema).includes('x-forklaunch-date-codec')) return value;
  if (depth > 64) throw new TypeError('Date codec recursion limit exceeded');
  if (typeof schema !== 'object' || schema === null) return value;
  if (schema.$ref !== undefined) {
    if (typeof schema.$ref !== 'string' || !schema.$ref.startsWith('#/')) throw new TypeError('Unsupported date codec reference');
    let target: unknown = root;
    for (const part of schema.$ref.slice(2).split('/')) {
      const key = part.replace(/~1/g, '/').replace(/~0/g, '~');
      if (typeof target !== 'object' || target === null || !Object.prototype.hasOwnProperty.call(target, key)) throw new TypeError('Unresolved date codec reference');
      target = (target as Record<string, unknown>)[key];
    }
    return portableDates(target as AnySchema, value, encode, root, depth + 1);
  }
  const codec = schema['x-forklaunch-date-codec'];
  if (codec !== undefined) {
    if (codec.version !== 1 || codec.kind !== 'utc-milliseconds' || !['input', 'value'].includes(codec.projection)) throw new TypeError('Unsupported date codec');
    if (encode) {
      if (!(value instanceof Date) || !Number.isFinite(Date.prototype.getTime.call(value))) throw new TypeError('Expected a valid Date');
      const wire = Date.prototype.toISOString.call(value);
      if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:[0-5]\d\.\d{3}Z$/.test(wire)) throw new TypeError('Date outside portable year range');
      return wire;
    }
    const pattern = codec.projection === 'input' ? /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:[0-5]\d(?:\.\d{1,3})?Z$/ : /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:[0-5]\d\.\d{3}Z$/;
    if (typeof value !== 'string' || !pattern.test(value)) throw new TypeError('Invalid UTC millisecond timestamp');
    const result = new Date(value);
    if (!Number.isFinite(result.getTime()) || result.toISOString().slice(0,19) !== value.slice(0,19)) throw new TypeError('Invalid calendar date');
    return result;
  }
  const alternatives = schema.oneOf ?? schema.anyOf;
  if (Array.isArray(alternatives)) {
    for (const candidate of alternatives) {
      try {
        const transformed = encode ? portableDates(candidate, value, true, root, depth + 1) : value;
        if (socketValidator({ ...candidate, $defs: (root as any).$defs, definitions: (root as any).definitions })(transformed)) return encode ? transformed : portableDates(candidate, value, false, root, depth + 1);
      } catch { /* Other alternatives may match. */ }
    }
    throw new TypeError('No date codec union alternative matches');
  }
  if (Array.isArray(schema.allOf)) {
    for (const candidate of schema.allOf) value = portableDates(candidate, value, encode, root, depth + 1);
  }
  if (Array.isArray(value)) return value.map((item, index) => portableDates(schema.prefixItems?.[index] ?? (Array.isArray(schema.items) ? schema.items[index] : schema.items) ?? true, item, encode, root, depth + 1));
  if (typeof value === 'object' && value !== null && !(value instanceof Date)) {
    const output: Record<string,unknown> = Object.create(null);
    for (const [key, item] of Object.entries(value)) output[key] = portableDates(schema.properties?.[key] ?? schema.additionalProperties ?? true, item, encode, root, depth + 1);
    return output;
  }
  return value;
}
