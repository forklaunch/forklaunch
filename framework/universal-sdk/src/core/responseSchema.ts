import type Ajv from 'ajv';
import type { AnySchema, ValidateFunction } from 'ajv';
import { OpenAPIObject } from 'openapi3-ts/oas31';
import { ajvSchema } from './ajvSchema';
import { responseContext, assertResponseEngine, assertResponseSchema, engineDialect, ResponseAjvFactory, ResponseSchemaContext } from './responseDialect';
import { coerceSpecialTypes } from './coerceSpecialTypes';
import { coerceReferenced, SchemaCursor } from './coerceReferenced';

let nextDocumentId = 0;

/**
 * Resolves response references against their document, without remote loading.
 * SDK schema inspection (including inline schemas) is bounded to 10,000 nodes
 * and depth 64. Referenced conversion supports JSON Pointers and registered
 * schema-ID resources, not anchor fragments or remote loading. The separate
 * legacy two-argument coerceSpecialTypes helper retains its existing behavior.
 */
export class ResponseSchemas {
  private positional = new WeakMap<object, Map<string, boolean>>();
  private inline = new WeakMap<object, Map<string, AnySchema>>();
  private documents = new WeakMap<OpenAPIObject, Map<string, { ajv: Ajv; base: string; context: ResponseSchemaContext }>>();
  constructor(
    private readonly source: Ajv,
    private readonly factory?: ResponseAjvFactory
  ) {}

  coerce(
    document: OpenAPIObject,
    pointer: string,
    schema: AnySchema,
    input: Record<string, unknown>
  ): unknown {
    const context = responseContext(document, schema, this.source);
    const referenced = hasReferences(schema);
    const engine = context.explicit && this.factory
      ? this.register(document, context).ajv : this.source;
    assertResponseEngine(engine, context);
    let positional = false;
    if (!referenced && typeof schema === 'object') {
      let cached = this.positional.get(schema);
      const previous = cached?.get(context.dialect);
      if (previous !== undefined) positional = previous;
      else {
        ajvSchema(schema, engine.opts?.schemaId, false, context.dialect, (node) => {
          assertResponseSchema(node, context);
          if (context.dialect === 'https://json-schema.org/draft/2020-12/schema'
            ? Array.isArray(node.prefixItems) : Array.isArray(node.items)) positional = true;
        });
        if (!cached) { cached = new Map(); this.positional.set(schema, cached); }
        cached.set(context.dialect, positional);
      }
    }
    if (!referenced && !positional)
      return coerceSpecialTypes(input, schema as Record<string, unknown>);
    const entry = referenced ? this.register(document, context) : {
      base: 'https://forklaunch.invalid/inline-response', ajv: engine, context
    };
    const locate = (uri: string): SchemaCursor => {
      const url = new URL(uri);
      const fragment = decodeURIComponent(url.hash.slice(1));
      url.hash = '';
      const resource = url.href;
      let current: unknown = entry.ajv.getSchema(resource)?.schema;
      if (current === undefined)
        throw new Error('Unresolved response coercion resource');
      let base = resource;
      if (fragment && !fragment.startsWith('/'))
        throw new Error('Response coercion supports JSON Pointer references');
      for (const escaped of fragment ? fragment.slice(1).split('/') : []) {
        const key = escaped.replace(/~1/g, '/').replace(/~0/g, '~');
        if (
          typeof current !== 'object' ||
          current === null ||
          !Object.hasOwn(current, key)
        )
          throw new Error('Unresolved response coercion pointer');
        current = (current as Record<string, unknown>)[key];
        if (
          typeof current === 'object' &&
          current !== null &&
          '$id' in current &&
          typeof current.$id === 'string'
        )
          base = new URL(current.$id, base).href;
      }
      return { schema: current, base, dialect: entry.context.dialect };
    };
    return coerceReferenced(input, referenced ? locate(`${entry.base}#${pointer}`) : {schema, base:entry.base, dialect:context.dialect}, {
      reference: (cursor, ref) => locate(new URL(ref, cursor.base).href),
      child: (cursor, child) => ({
        schema: child,
        dialect: cursor.dialect,
        base:
          typeof child === 'object' &&
          child !== null &&
          '$id' in child &&
          typeof child.$id === 'string'
            ? new URL(child.$id, cursor.base).href
            : cursor.base
      })
    });
  }

  validator(
    document: OpenAPIObject,
    pointer: string,
    schema: AnySchema
  ): ValidateFunction {
    const context = responseContext(document, schema, this.source);
    if (!hasReferences(schema)) {
      const engine = context.explicit && this.factory
        ? this.register(document, context).ajv : this.source;
      assertResponseEngine(engine, context);
      let cached = typeof schema === 'object' ? this.inline.get(schema) : undefined;
      let compiled = cached?.get(context.dialect);
      if (compiled === undefined) {
        compiled = ajvSchema(schema, engine.opts?.schemaId, false, context.dialect,
          (node) => assertResponseSchema(node, context));
        if (typeof schema === 'object') {
          if (!cached) { cached = new Map(); this.inline.set(schema, cached); }
          cached.set(context.dialect, compiled);
        }
      }
      const validationSchema = compiled;
      const validate = ((data: unknown) => {
        const valid = engine.validate(validationSchema, data);
        validate.errors = engine.errors;
        return valid;
      }) as ValidateFunction;
      return validate;
    }
    const entry = this.register(document, context);
    const validator = entry.ajv.getSchema(`${entry.base}#${pointer}`);
    if (!validator)
      throw new Error('Response schema not found in OpenAPI document');
    return validator;
  }
  private register(document: OpenAPIObject, selected = responseContext(document, {}, this.source)): { ajv: Ajv; base: string; context: ResponseSchemaContext } {
    let contexts = this.documents.get(document);
    if (!contexts) { contexts = new Map(); this.documents.set(document, contexts); }
    let entry = contexts.get(selected.dialect);
    if (!entry) {
      // Separate registries may reuse the same absolute schema identifiers.
      // An instance per immutable registry document prevents cache cross-talk.
      // Direct constructor callers retain their complete supplied validator,
      // including custom keywords. Without a factory, conflicting explicit IDs
      // deliberately fail rather than replacing another document's schemas.
      const ajv = this.factory ? this.factory(Object.freeze({ ...selected })) : this.source;
      assertResponseEngine(ajv, selected);
      const context = selected.explicit ? selected : {...selected, dialect:engineDialect(ajv)};
      const base =
        (document as OpenAPIObject & { $id?: string }).$id ??
        `https://forklaunch.invalid/sdk-document/${++nextDocumentId}`;
      const dialect = ajv.defaultMeta?.();
      const compilation = ajvSchema(document, ajv.opts?.schemaId, true, typeof dialect === 'string' ? dialect : undefined, (node) => assertResponseSchema(node, context)) as OpenAPIObject;
      for (const schema of Object.values(compilation.components?.schemas ?? {})) {
        if (
          schema &&
          typeof schema === 'object' &&
          '$id' in schema &&
          typeof schema.$id === 'string'
        )
          ajv.addSchema(schema as AnySchema, new URL(schema.$id, base).href);
      }
      ajv.addSchema(compilation, base);
      entry = { ajv, base, context };
      contexts.set(selected.dialect, entry);
    }
    return entry;
  }
}

export function schemaPointer(
  path: string,
  method: string,
  status: number,
  contentType: string
): string {
  return [
    'paths',
    path,
    method.toLowerCase(),
    'responses',
    String(status),
    'content',
    contentType,
    'schema'
  ]
    .map((part) => `/${part.replace(/~/g, '~0').replace(/\//g, '~1')}`)
    .join('');
}

function hasReferences(value: unknown): boolean {
  const pending: Array<[unknown, number]> = [[value, 0]];
  const seen = new Set<object>();
  let nodes = 0;
  while (pending.length) {
    const [current, depth] = pending.pop()!;
    if (++nodes > 10000 || depth > 64)
      throw new Error('Response schema traversal budget exceeded');
    if (typeof current !== 'object' || current === null || seen.has(current))
      continue;
    if ('$ref' in current || '$id' in current) return true;
    seen.add(current);
    const children = Object.values(current);
    if (children.length + pending.length > 10000)
      throw new Error('Response schema traversal budget exceeded');
    pending.push(
      ...children.map((child) => [child, depth + 1] as [unknown, number])
    );
  }
  return false;
}
