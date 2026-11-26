import { coerceSpecialTypes } from './coerceSpecialTypes';

export interface SchemaCursor {
  schema: unknown;
  base: string;
  dialect?: string;
}
export interface SchemaResolver {
  reference(cursor: SchemaCursor, ref: string): SchemaCursor;
  child(cursor: SchemaCursor, schema: unknown): SchemaCursor;
}
const record = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/**
 * Immutable conversion of referenced properties/items and root strings/arrays.
 * Limits: depth 64, 10,000 value/schema visits and reference resolutions, and
 * 256 references per chain. Exceeding a budget throws before a result escapes.
 * Reference-only cycles and structural/conditional or conflicting compositions
 * remain unconverted; string unions convert only when non-null branches agree.
 * This does not replace JSON Schema validation or expand arbitrary schemas.
 */
export function coerceReferenced(
  input: unknown,
  root: SchemaCursor,
  resolver: SchemaResolver
): unknown {
  let nodes = 0;
  let references = 0;
  const follow = (start: SchemaCursor): SchemaCursor | undefined => {
    let cursor = start;
    const seen = new Set<string>();
    while (record(cursor.schema) && typeof cursor.schema.$ref === 'string') {
      if (++references > 10000 || seen.size >= 256)
        throw new Error('Response coercion reference budget exceeded');
      if (
        Object.keys(cursor.schema).some(
          (key) =>
            ![
              '$ref',
              '$id',
              'title',
              'description',
              '$comment',
              'examples',
              'default',
              'deprecated',
              'readOnly',
              'writeOnly'
            ].includes(key)
        )
      )
        return undefined;
      const identity = new URL(cursor.schema.$ref, cursor.base).href;
      if (seen.has(identity)) return undefined;
      seen.add(identity);
      cursor = resolver.reference(cursor, cursor.schema.$ref);
    }
    return cursor;
  };
  const convertString = (value: string, format: unknown): unknown => {
    if (format !== 'date-time' && format !== 'binary') return value;
    const result = coerceSpecialTypes(
      { value },
      { properties: { value: { type: 'string', format } } }
    );
    return (result as { value: unknown }).value;
  };
  const visit = (
    value: unknown,
    initial: SchemaCursor,
    depth: number
  ): unknown => {
    if (++nodes > 10000 || depth > 64)
      throw new Error('Response coercion traversal budget exceeded');
    const cursor = follow(initial);
    if (!cursor || !record(cursor.schema) || value === null) return value;
    const schema = cursor.schema;
    // Structural/conditional composition is deliberately not inferred. For
    // string unions, all non-null alternatives must agree on conversion.
    const union = schema.anyOf ?? schema.oneOf;
    if (Array.isArray(union)) {
      if (typeof value !== 'string') return value;
      const formats: unknown[] = [];
      for (const alternative of union) {
        if (++nodes > 10000)
          throw new Error('Response coercion traversal budget exceeded');
        const branch = follow(resolver.child(cursor, alternative));
        if (!branch || !record(branch.schema)) return value;
        if (branch.schema.type === 'null') continue;
        if (branch.schema.type !== 'string') return value;
        formats.push(branch.schema.format);
      }
      return formats.length && formats.every((format) => format === formats[0])
        ? convertString(value, formats[0])
        : value;
    }
    if ('allOf' in schema || 'if' in schema || 'not' in schema) return value;
    if (
      typeof value === 'string' &&
      (schema.type === 'string' ||
        (Array.isArray(schema.type) && schema.type.includes('string')))
    )
      return convertString(value, schema.format);
    if (Array.isArray(value)) {
      const modern = cursor.dialect?.replace(/#$/, '') === 'https://json-schema.org/draft/2020-12/schema';
      const prefix = modern
        ? (Array.isArray(schema.prefixItems) ? schema.prefixItems : undefined)
        : (Array.isArray(schema.items) ? schema.items : undefined);
      const tail = prefix && !modern ? schema.additionalItems : schema.items;
      if (!prefix && !record(tail)) return value;
      if (nodes + value.length > 10000)
        throw new Error('Response coercion traversal budget exceeded');
      return value.map((item, index) => {
        const itemSchema = prefix && index < prefix.length ? prefix[index] : tail;
        return record(itemSchema)
          ? visit(item, resolver.child(cursor, itemSchema), depth + 1)
          : item;
      });
    }
    if (record(value) && record(schema.properties)) {
      for (const name in value) {
        if (Object.hasOwn(value, name) && ++nodes > 10000)
          throw new Error('Response coercion traversal budget exceeded');
      }
      const output = { ...value };
      for (const name in schema.properties) {
        if (!Object.hasOwn(schema.properties, name)) continue;
        if (++nodes > 10000)
          throw new Error('Response coercion traversal budget exceeded');
        if (Object.hasOwn(value, name))
          output[name] = visit(
            value[name],
            resolver.child(cursor, schema.properties[name]),
            depth + 1
          );
      }
      return output;
    }
    return value;
  };
  return visit(input, root, 0);
}
