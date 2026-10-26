import type Ajv from 'ajv';
import type { AnySchema } from 'ajv';
import type { OpenAPIObject } from 'openapi3-ts/oas31';

export interface ResponseSchemaContext {
  readonly dialect: string;
  readonly explicit: boolean;
}
export type ResponseAjvFactory = (context: ResponseSchemaContext) => Ajv;
export const draft7 = 'http://json-schema.org/draft-07/schema';
export const draft2020 = 'https://json-schema.org/draft/2020-12/schema';
export function canonicalDialect(value: string): string {
  return value.replace(/#$/, '').replace(/^https:\/\/json-schema.org\/draft-0/, 'http://json-schema.org/draft-0');
}
export function engineDialect(engine: Ajv): string {
  const meta = engine.defaultMeta?.();
  return canonicalDialect(typeof meta === 'string' ? meta : draft7);
}
export function responseContext(document: OpenAPIObject, schema: AnySchema, engine: Ajv): ResponseSchemaContext {
  const local = typeof schema === 'object' && typeof schema.$schema === 'string' ? canonicalDialect(schema.$schema) : undefined;
  const declared = 'jsonSchemaDialect' in document && typeof document.jsonSchemaDialect === 'string' ? canonicalDialect(document.jsonSchemaDialect) : undefined;
  if (local && declared && local !== declared) throw new Error('Response schema dialect conflicts with its OpenAPI document');
  return {dialect:local ?? declared ?? engineDialect(engine), explicit:local !== undefined || declared !== undefined};
}
export function assertResponseEngine(engine: Ajv, context: ResponseSchemaContext): void {
  if (context.explicit && engineDialect(engine) !== context.dialect) {
    throw new Error(`Response AJV engine does not support selected dialect ${context.dialect}; supply a matching response factory`);
  }
}
export function assertResponseSchema(schema: Record<string, unknown>, context: ResponseSchemaContext): void {
  if (typeof schema.$schema === 'string' && canonicalDialect(schema.$schema) !== context.dialect) {
    throw new Error('Mixed response schema resource dialects are not supported');
  }
}
