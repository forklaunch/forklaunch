import { SchemaObject } from 'openapi3-ts/oas31';
import { z, ZodType } from 'zod/v3';

/** Root input wrappers only. Never evaluate defaults, refinements or transforms. */
export function requestBodyOpenApi(
  schema: ZodType,
  exportSchema: (schema: ZodType) => SchemaObject
): { required: boolean; schema: SchemaObject } {
  function visit(
    input: ZodType,
    depth: number
  ): { optional: boolean; input: ZodType } {
    if (depth > 64) throw new Error('Request body wrapper depth exceeds 64');
    const def = input._def as {
      typeName: string;
      innerType?: ZodType;
      schema?: ZodType;
      effect?: { type: string };
    };
    if (def.typeName === 'ZodOptional' || def.typeName === 'ZodDefault') {
      return { ...visit(def.innerType!, depth + 1), optional: true };
    }
    if (def.typeName === 'ZodNullable') {
      const inner = visit(def.innerType!, depth + 1);
      return { optional: inner.optional, input: z.nullable(inner.input) };
    }
    if (def.typeName === 'ZodEffects') {
      const inner = visit(def.schema!, depth + 1);
      // Preprocessors/refinements may change admission. Their arbitrary code is
      // not evaluated for metadata; conservatively retain requiredness.
      return {
        ...inner,
        optional: def.effect?.type === 'transform' && inner.optional
      };
    }
    return {
      optional: ['ZodUndefined', 'ZodVoid', 'ZodAny', 'ZodUnknown'].includes(
        def.typeName
      ),
      input
    };
  }
  const projected = visit(schema, 0);
  const result = exportSchema(projected.input);
  if (schema.description !== undefined) result.description = schema.description;
  return { required: !projected.optional, schema: result };
}
