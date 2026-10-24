import { diagnosticProvenance, zodDiagnosticsKeyword, type ZodDiagnosticOptions } from './zodDiagnostics';
import { projectRegexChecks, zodRegexKeyword } from './zodRegex';
import type { SchemaObject, SchemaObjectType } from 'openapi3-ts/oas31';
import { merge } from './schemaMerge';
import { assertPureInputSchema, pureInputOptional } from './pureInput';
import { stringLiteralPattern } from './stringLiteral';
import { projectStringPredicates, projectStringChecks, projectNumberChecks, projectArrayChecks, zodChecksKeyword } from './zodChecks';
import { AnyZodObject, z, ZodTypeAny } from 'zod/v3';
import {
  factoryEmailPattern,
  hasFactoryEmailGrammar
} from '../../src/zod/emailOpenApi';

import {
  factoryUriFormat,
  hasFactoryUriGrammar
} from '../../src/zod/uriOpenApi';

export type AnatineSchemaObject = SchemaObject & {
  hideDefinitions?: string[];
  $schema?: string;
};

export interface OpenApiZodAny extends ZodTypeAny {
  metaOpenApi?: AnatineSchemaObject | AnatineSchemaObject[];
}

interface OpenApiZodAnyObject extends AnyZodObject {
  metaOpenApi?: AnatineSchemaObject | AnatineSchemaObject[];
}

type OpenAPIVersion = '3.0' | '3.1';

export interface SchemaGenerationOptions extends ZodDiagnosticOptions {
  /** Retain trusted ordered runtime schema provenance; implies callback-free input projection. */
  readonly zodDiagnostics?: boolean;
  readonly jsonSchemaDialect: 'https://json-schema.org/draft/2020-12/schema';
  /** Reject callback-dependent schemas and omit computed defaults recursively. */
  readonly pureInput?: boolean;
  /** Emit reserved exact UTF-16 length, array cardinality and Zod remainder checks. Consumers must support this profile. */
  readonly zodChecks?: boolean;
  /** Emit ordered ECMAScript regex rules; consumers require an explicit execution policy. */
  readonly zodRegex?: boolean;
}
interface ParsingArgs<T> {
  zodChecks?: boolean;
  zodRegex?: boolean;
  pureInput?: { isOptional: (schema: ZodTypeAny) => boolean };
  generateSchema: (
    schema: OpenApiZodAny,
    useOutput?: boolean,
    openApiVersion?: OpenAPIVersion
  ) => SchemaObject;
  jsonSchemaDialect?: SchemaGenerationOptions['jsonSchemaDialect'];
  zodRef: T;
  schemas: AnatineSchemaObject[];
  useOutput?: boolean;
  hideDefinitions?: string[];
  openApiVersion: OpenAPIVersion;
}

export function extendApi<T extends OpenApiZodAny>(
  schema: T,
  schemaObject: AnatineSchemaObject = {}
): T {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const This = (schema as any).constructor;
  const newSchema = new This(schema._def);
  newSchema.metaOpenApi = Object.assign(
    {},
    schema.metaOpenApi || {},
    schemaObject
  );
  return newSchema;
}

function iterateZodObject({
  generateSchema,
  zodRef,
  useOutput,
  hideDefinitions,
  openApiVersion
}: ParsingArgs<OpenApiZodAnyObject>) {
  const reduced = Object.keys(zodRef.shape)
    .filter((key) => hideDefinitions?.includes(key) === false)
    .reduce(
      (carry, key) => ({
        ...carry,
        [key]: generateSchema(zodRef.shape[key], useOutput, openApiVersion)
      }),
      {} as Record<string, SchemaObject>
    );

  return reduced;
}

function typeFormat<const T extends SchemaObjectType>(
  type: T,
  openApiVersion: OpenAPIVersion
) {
  return openApiVersion === '3.0' ? type : [type];
}

function parseTransformation({
  generateSchema,
  zodRef,
  schemas,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodTransformer<never> | z.ZodEffects<never>>): SchemaObject {
  const input = generateSchema(zodRef._def.schema, useOutput, openApiVersion);

  let output = 'undefined';
  if (useOutput && zodRef._def.effect) {
    const effect =
      zodRef._def.effect.type === 'transform' ? zodRef._def.effect : null;
    if (effect && 'transform' in effect) {
      try {
        const type = Array.isArray(input.type) ? input.type[0] : input.type;
        output = typeof effect.transform(
          ['integer', 'number'].includes(`${type}`)
            ? 0
            : 'string' === type
              ? ''
              : 'boolean' === type
                ? false
                : 'object' === type
                  ? {}
                  : 'null' === type
                    ? null
                    : 'array' === type
                      ? []
                      : undefined,
          { addIssue: () => undefined, path: [] } // TODO: Discover if context is necessary here
        );
      } catch {
        /**/
      }
    }
  }
  const outputType = output as 'number' | 'string' | 'boolean' | 'null';
  return merge(
    {
      ...(zodRef.description ? { description: zodRef.description } : {}),
      ...input,
      ...(['number', 'string', 'boolean', 'null'].includes(output)
        ? {
            type: typeFormat(outputType, openApiVersion)
          }
        : {})
    },
    ...schemas
  );
}

function parseString({
  zodChecks,
  zodRegex,
  zodRef,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodString>): SchemaObject {
  const baseSchema: SchemaObject = {
    type: typeFormat('string', openApiVersion)
  };
  const { checks = [] } = zodRef._def;
  const regexProfile = zodRegex ? projectRegexChecks(checks, schemas as Record<string, unknown>[]) : undefined;
  const exact = zodChecks ? projectStringChecks(checks, schemas as Record<string, unknown>[], hasFactoryEmailGrammar(zodRef)) : undefined;
  const predicates = zodChecks ? projectStringPredicates(checks, schemas as Record<string, unknown>[], Object.values(exact ?? {}).reduce<number>((count, value)=>count + (Array.isArray(value) ? value.length : 0),0)) : undefined;
  checks.forEach((item) => {
    switch (item.kind) {
      case 'email':
        if (hasFactoryEmailGrammar(zodRef)) {
          baseSchema.pattern = factoryEmailPattern;
        } else {
          baseSchema.format = 'email';
        }
        break;
      case 'uuid':
        baseSchema.format = 'uuid';
        break;
      case 'cuid':
        baseSchema.format = 'cuid';
        break;
      case 'url':
        baseSchema.format = hasFactoryUriGrammar(zodRef)
          ? factoryUriFormat
          : 'uri';
        break;
      case 'datetime':
        baseSchema.format = 'date-time';
        break;
      case 'length':
        baseSchema.minLength = item.value;
        baseSchema.maxLength = item.value;
        break;
      case 'max':
        baseSchema.maxLength = item.value;
        break;
      case 'min':
        baseSchema.minLength = item.value;
        break;
      case 'regex':
        if (!regexProfile) baseSchema.pattern = item.regex.source;
        break;
    }
  });
  if (!checks.some(item =>
    item.kind === 'trim' || item.kind === 'toLowerCase' || item.kind === 'toUpperCase'
  )) {
    const patterns = checks.map(stringLiteralPattern).filter(pattern => pattern !== undefined);
    if (patterns.length) baseSchema.allOf = patterns.map(pattern => ({ pattern }));
  }
  if (predicates) baseSchema.allOf = [...(baseSchema.allOf ?? []), { [zodChecksKeyword]: predicates }];
  // Zod applies every declared length rule. Collapse only the non-transforming
  // finite registration profile; UTF-16 versus JSON Schema code-point counting
  // remains a separate wire-schema limitation, even for a single constraint.
  const bounds = checks.filter(item =>
    item.kind === 'min' || item.kind === 'max' || item.kind === 'length'
  );
  if (!checks.some(item =>
    item.kind === 'trim' || item.kind === 'toLowerCase' || item.kind === 'toUpperCase'
  ) && bounds.every(item => Number.isSafeInteger(item.value) && item.value >= 0)) {
    delete baseSchema.minLength;
    delete baseSchema.maxLength;
    for (const item of bounds) {
      if (item.kind === 'min' || item.kind === 'length') {
        baseSchema.minLength = Math.max(baseSchema.minLength ?? 0, item.value);
      }
      if (item.kind === 'max' || item.kind === 'length') {
        baseSchema.maxLength = Math.min(baseSchema.maxLength ?? item.value, item.value);
      }
    }
  }
  if (zodChecks) {
    if (exact?.dateTimes?.length && baseSchema.format === 'date-time') delete baseSchema.format;
    if (exact?.formats?.some(format => format !== 'email' || !hasFactoryEmailGrammar(zodRef)) && ['email', 'uuid', 'uri', 'cuid', factoryUriFormat].includes(baseSchema.format as string)) delete baseSchema.format;
    if (exact?.formats?.includes('email') && hasFactoryEmailGrammar(zodRef)) delete baseSchema.pattern;
    if (exact?.utf16MinLength !== undefined || exact?.utf16MinLengths !== undefined) delete baseSchema.minLength;
    if (exact?.utf16MaxLength !== undefined || exact?.utf16MaxLengths !== undefined) delete baseSchema.maxLength;
    if (exact) Object.assign(baseSchema, { [zodChecksKeyword]: exact });
  }
  if (regexProfile) Object.assign(baseSchema, { [zodRegexKeyword]: regexProfile });
  return merge(
    baseSchema,
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseNumber({
  zodChecks,
  zodRef,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodNumber>): SchemaObject {
  const baseSchema: SchemaObject = {
    type: typeFormat('number', openApiVersion)
  };
  const { checks = [] } = zodRef._def;
  const exact = zodChecks ? projectNumberChecks(checks, schemas as Record<string, unknown>[]) : undefined;
  checks.forEach((item) => {
    switch (item.kind) {
      case 'max':
        if (item.inclusive || openApiVersion === '3.0') {
          baseSchema.maximum = item.value;
        }
        if (!item.inclusive) {
          if (openApiVersion === '3.0') {
            // exclusiveMaximum has conflicting types in oas31 and oas30
            baseSchema.exclusiveMaximum = true as unknown as number;
          } else {
            baseSchema.exclusiveMaximum = item.value;
          }
        }
        break;
      case 'min':
        if (item.inclusive || openApiVersion === '3.0') {
          baseSchema.minimum = item.value;
        }
        if (!item.inclusive) {
          if (openApiVersion === '3.0') {
            // exclusiveMinimum has conflicting types in oas31 and oas30
            baseSchema.exclusiveMinimum = true as unknown as number;
          } else {
            baseSchema.exclusiveMinimum = item.value;
          }
        }
        break;
      case 'int':
        baseSchema.type = typeFormat('integer', openApiVersion);
        break;
      case 'multipleOf':
        baseSchema.multipleOf = item.value;
        break;
    }
  });
  // Zod runs every numeric bound check. Preserve their conjunction rather than
  // whichever check happened to be declared last. Non-finite registrations
  // retain the existing exporter boundary; they are not JSON numeric bounds.
  const bounds = checks.filter(
    (item) => item.kind === 'min' || item.kind === 'max'
  );
  if (bounds.every((item) => Number.isFinite(item.value))) {
    delete baseSchema.minimum;
    delete baseSchema.maximum;
    delete baseSchema.exclusiveMinimum;
    delete baseSchema.exclusiveMaximum;
    for (const kind of ['min', 'max'] as const) {
      const strongest = bounds
        .filter((item) => item.kind === kind)
        .reduce<{ value: number; inclusive: boolean } | undefined>(
          (selected, item) => {
            if (
              !selected ||
              (kind === 'min'
                ? item.value > selected.value
                : item.value < selected.value)
            )
              return item;
            if (item.value === selected.value && !item.inclusive) return item;
            return selected;
          },
          undefined
        );
      if (!strongest) continue;
      const bound = kind === 'min' ? 'minimum' : 'maximum';
      const exclusive =
        kind === 'min' ? 'exclusiveMinimum' : 'exclusiveMaximum';
      if (strongest.inclusive || openApiVersion === '3.0')
        baseSchema[bound] = strongest.value;
      if (!strongest.inclusive)
        baseSchema[exclusive] =
          openApiVersion === '3.0'
            ? (true as unknown as number)
            : strongest.value;
    }
  }
  if (zodChecks) {
    // In the admitted finite-JSON number domain, nonfinite registrations can
    // be represented exactly without serializing NaN/Infinity as null bounds.
    delete baseSchema.minimum;
    delete baseSchema.maximum;
    delete baseSchema.exclusiveMinimum;
    delete baseSchema.exclusiveMaximum;
    let impossible = false;
    for (const check of bounds) {
      const keyword = check.kind === 'min'
        ? (check.inclusive ? 'minimum' : 'exclusiveMinimum')
        : (check.inclusive ? 'maximum' : 'exclusiveMaximum');
      if (schemas.some((metadata) =>
        Object.prototype.hasOwnProperty.call(metadata, keyword)
      )) continue;
      if (typeof check.value !== 'number')
        throw new Error('Zod numeric bounds must be numbers');
      if (Number.isNaN(check.value)) continue;
      if (!Number.isFinite(check.value)) {
        impossible ||= check.kind === 'min'
          ? check.value === Infinity
          : check.value === -Infinity;
        continue;
      }
      const previous = baseSchema[keyword];
      baseSchema[keyword] = previous === undefined
        ? check.value
        : check.kind === 'min'
          ? Math.max(previous, check.value)
          : Math.min(previous, check.value);
    }
    // Keep impossible source bounds conjunctive with unrelated annotations.
    // Explicit annotations for the original bound keyword were handled above.
    // Like ordinary numeric keywords, this assertion is inert for other
    // types (notably the general exporter's merged nullable type).
    if (impossible) baseSchema.allOf = [{ not: { type: 'number' } }];
    if (exact) {
      delete baseSchema.multipleOf;
      Object.assign(baseSchema, { [zodChecksKeyword]: exact });
    }
  }
  return merge(
    baseSchema,
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function getExcludedDefinitionsFromSchema(
  schemas: AnatineSchemaObject[]
): string[] {
  const excludedDefinitions = [];
  for (const schema of schemas) {
    if (Array.isArray(schema.hideDefinitions)) {
      excludedDefinitions.push(...schema.hideDefinitions);
    }
  }

  return excludedDefinitions;
}

function parseObject({
  generateSchema,
  pureInput,
  zodRef,
  schemas,
  useOutput,
  hideDefinitions,
  openApiVersion
}: ParsingArgs<
  z.ZodObject<never, 'passthrough' | 'strict' | 'strip'>
>): SchemaObject {
  let additionalProperties: SchemaObject['additionalProperties'];

  // `catchall` obviates `strict`, `strip`, and `passthrough`
  if (!(
    zodRef._def.catchall instanceof z.ZodNever ||
    zodRef._def.catchall?._def.typeName === 'ZodNever'
  ))
    additionalProperties = generateSchema(
      zodRef._def.catchall,
      useOutput,
      openApiVersion
    );
  else if (zodRef._def.unknownKeys === 'passthrough')
    additionalProperties = true;
  else if (zodRef._def.unknownKeys === 'strict') additionalProperties = false;

  // So that `undefined` values don't end up in the schema and be weird
  additionalProperties =
    additionalProperties != null ? { additionalProperties } : {};

  const requiredProperties = Object.keys(
    (zodRef as z.AnyZodObject).shape
  ).filter((key) => {
    const item = (zodRef as z.AnyZodObject).shape[key];
    if (pureInput) return !pureInput.isOptional(item);
    return !(
      item.isOptional() ||
      item instanceof z.ZodDefault ||
      item._def.typeName === 'ZodDefault'
    );
  });

  const required =
    requiredProperties.length > 0 ? { required: requiredProperties } : {};

  return merge(
    {
      type: typeFormat('object', openApiVersion),
      properties: iterateZodObject({
        generateSchema,
        zodRef: zodRef as OpenApiZodAnyObject,
        schemas,
        useOutput,
        hideDefinitions: getExcludedDefinitionsFromSchema(schemas),
        openApiVersion
      }),
      ...required,
      ...additionalProperties,
      ...hideDefinitions
    },
    zodRef.description
      ? { description: zodRef.description, hideDefinitions }
      : {},
    ...schemas
  );
}

// A finite set of literal input strings, never a projection of transformed keys.
function recordStringKeys(schema: ZodTypeAny): string[] | undefined {
  let remaining = 10000;
  function visit(input: ZodTypeAny, depth: number): string[] | undefined {
    if (--remaining < 0 || depth > 64) return undefined;
    const def = input._def as z.ZodTypeDef & {
      typeName: string;
      value?: unknown;
      values: string[];
      innerType: ZodTypeAny;
      options: ZodTypeAny[];
    };
    switch (def.typeName) {
      case 'ZodLiteral':
        return typeof def.value === 'string' ? [def.value] : undefined;
      case 'ZodEnum':
        return def.values.length <= remaining &&
          def.values.every((value: unknown) => typeof value === 'string')
          ? ((remaining -= def.values.length), def.values)
          : undefined;
      case 'ZodOptional':
      case 'ZodNullable':
        return visit(def.innerType, depth + 1);
      case 'ZodUnion': {
        if (def.options.length > remaining) return undefined;
        const keys: string[] = [];
        for (const option of def.options) {
          const branch = visit(option, depth + 1);
          if (branch === undefined) return undefined;
          keys.push(...branch);
        }
        return keys;
      }
      default:
        return undefined;
    }
  }
  const keys = visit(schema, 0);
  return keys === undefined ? undefined : [...new Set(keys)];
}

// Property names are always strings. Restrict the generated schema to that
// domain before handing it to coercing engines (AJV cannot safely coerce its
// immutable property-name loop variable). Annotation data is not traversed.
function recordInputKeySchema(schema: SchemaObject): SchemaObject {
  let remaining = 10000;
  const visit = (input: unknown, depth: number): unknown => {
    if (depth > 64 || --remaining < 0)
      throw new Error('Pure record key projection budget exceeded');
    if (typeof input !== 'object' || input === null) return input;
    const node = input as Record<string, unknown>;
    if ('$ref' in node || '$dynamicRef' in node || '$recursiveRef' in node)
      throw new Error(
        'Pure record key references require resolved string-domain schemas'
      );
    const output = { ...node };
    if (node.type !== undefined) {
      const types = Array.isArray(node.type) ? node.type : [node.type];
      if (
        !types.length ||
        types.some((type) =>
          !['null', 'boolean', 'object', 'array', 'number', 'string', 'integer'].includes(type as string)
        ) ||
        new Set(types).size !== types.length
      )
        throw new Error('Invalid pure record key schema type annotation');
      if (!types.includes('string')) return { not: {} };
      output.type = 'string';
    }
    if (Array.isArray(node.enum)) {
      if (!node.enum.length)
        throw new Error('Invalid empty record key enum annotation');
      const values = node.enum.filter((value) => typeof value === 'string');
      if (!values.length) return { not: {} };
      output.enum = values;
    }
    if (
      Object.prototype.hasOwnProperty.call(node, 'const') &&
      typeof node.const !== 'string'
    ) return { not: {} };
    for (const keyword of ['allOf', 'anyOf', 'oneOf']) {
      if (Array.isArray(node[keyword]))
        output[keyword] = node[keyword].map((child) => visit(child, depth + 1));
    }
    for (const keyword of ['not', 'if', 'then', 'else']) {
      if (Object.prototype.hasOwnProperty.call(node, keyword))
        output[keyword] = visit(node[keyword], depth + 1);
    }
    return output;
  };
  return visit(schema, 0) as SchemaObject;
}

function parseRecord({
  generateSchema,
  pureInput,
  zodRef,
  schemas,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodRecord>): SchemaObject {
  // Pure input is an explicit 2020-12 profile: retain the admitted key
  // schema, including constraints that a finite key-set cannot represent.
  const keySchema = pureInput
    ? generateSchema(zodRef._def.keyType, useOutput, openApiVersion)
    : undefined;
  const valueSchema =
    zodRef._def.valueType instanceof z.ZodUnknown
      ? {}
      : generateSchema(zodRef._def.valueType, useOutput, openApiVersion);
  const keys = recordStringKeys(zodRef._def.keyType);
  const constraints: SchemaObject = {};
  if (keySchema !== undefined) {
    constraints.propertyNames = recordInputKeySchema(keySchema);
  } else if (keys !== undefined) {
    if (openApiVersion === '3.1') {
      constraints.propertyNames = keys.length ? { enum: keys } : { not: {} };
    } else {
      constraints.properties = Object.fromEntries(
        keys.map((key) => [key, valueSchema])
      );
      constraints.additionalProperties = false;
    }
  }
  return merge(
    {
      type: typeFormat('object', openApiVersion),
      additionalProperties: valueSchema,
      ...constraints
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseBigInt({
  zodRef,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodBigInt>): SchemaObject {
  return merge(
    {
      type: typeFormat('integer', openApiVersion),
      format: 'int64'
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseBoolean({
  zodRef,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodBoolean>): SchemaObject {
  return merge(
    { type: typeFormat('boolean', openApiVersion) },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseDate({
  zodRef,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodDate>): SchemaObject {
  return merge(
    {
      type: typeFormat('string', openApiVersion),
      format: 'date-time'
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseNull({
  zodRef,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodNull>): SchemaObject {
  return merge(
    openApiVersion === '3.0'
      ? { type: 'null' as SchemaObjectType }
      : {
          type: ['string', 'null'] as SchemaObjectType[],
          enum: [null]
        },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseOptional({
  generateSchema,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodOptional<OpenApiZodAny>>): SchemaObject {
  return merge(
    generateSchema(zodRef.unwrap(), useOutput, openApiVersion),
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseNullable({
  generateSchema,
  pureInput,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodNullable<OpenApiZodAny>>): SchemaObject {
  const schema = generateSchema(zodRef.unwrap(), useOutput, openApiVersion);
  if (pureInput) return merge(
    { anyOf: [schema, { type: 'null' }] },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
  return merge(
    schema,
    openApiVersion === '3.0'
      ? { nullable: true }
      : { type: typeFormat('null', openApiVersion) },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseDefault({
  generateSchema,
  pureInput,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodDefault<OpenApiZodAny>>): SchemaObject {
  return merge(
    {
      ...(pureInput ? {} : { default: zodRef._def.defaultValue() }),
      ...generateSchema(zodRef._def.innerType, useOutput, openApiVersion)
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseArray({
  zodChecks,
  generateSchema,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodArray<OpenApiZodAny>>): SchemaObject {
  const constraints: SchemaObject = {};
  if (zodRef._def.exactLength != null) {
    constraints.minItems = zodRef._def.exactLength.value;
    constraints.maxItems = zodRef._def.exactLength.value;
  }

  if (zodRef._def.minLength != null)
    constraints.minItems = zodRef._def.minLength.value;
  if (zodRef._def.maxLength != null)
    constraints.maxItems = zodRef._def.maxLength.value;

  // Exact length and the retained min/max checks all run independently in Zod.
  // Keep contradictory bounds contradictory; invalid registrations remain an
  // explicit exporter boundary rather than being silently rounded or clamped.
  const lengths = [
    zodRef._def.exactLength,
    zodRef._def.minLength,
    zodRef._def.maxLength
  ]
    .filter((item) => item != null)
    .map((item) => item.value);
  if (lengths.every((value) => Number.isSafeInteger(value) && value >= 0)) {
    const exact = zodRef._def.exactLength?.value;
    const minimum = zodRef._def.minLength?.value;
    const maximum = zodRef._def.maxLength?.value;
    if (exact !== undefined) {
      constraints.minItems =
        minimum === undefined ? exact : Math.max(exact, minimum);
      constraints.maxItems =
        maximum === undefined ? exact : Math.min(exact, maximum);
    }
  }

  const exact = zodChecks ? projectArrayChecks(zodRef._def, schemas as Record<string, unknown>[]) : undefined;
  if (exact?.arrayMinItems) delete constraints.minItems;
  if (exact?.arrayMaxItems) delete constraints.maxItems;
  return merge(
    {
      ...(exact ? {[zodChecksKeyword]:exact} : {}),
      type: typeFormat('array', openApiVersion),
      items: generateSchema(zodRef.element, useOutput, openApiVersion),
      ...constraints
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseLiteral({
  pureInput,
  schemas,
  zodRef,
  openApiVersion
}: ParsingArgs<z.ZodLiteral<OpenApiZodAny>>): SchemaObject {
  const value = zodRef._def.value as unknown;
  if (pureInput && (value === null || value === undefined)) {
    return merge(
      value === null ? { type: 'null' } : { not: {} },
      zodRef.description ? { description: zodRef.description } : {},
      ...schemas
    );
  }
  const type = typeof zodRef._def.value as 'string' | 'number' | 'boolean';
  return merge(
    {
      type: typeFormat(type, openApiVersion),
      enum: [zodRef._def.value]
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseEnum({
  schemas,
  zodRef,
  openApiVersion
}: ParsingArgs<z.ZodEnum<never> | z.ZodNativeEnum<never>>): SchemaObject {
  if (zodRef._def.typeName === 'ZodNativeEnum') {
    // Use Zod's own reverse-map/private-key semantics; enum_ factory unions are
    // intentionally distinct and retain their existing projection.
    const values = [...new Set(z.util.getValidEnumValues(zodRef._def.values))];
    if (values.every((value) =>
      typeof value === 'string' ||
      (typeof value === 'number' && Number.isFinite(value))
    )) {
      const type =
        values.length && values.every((value) => typeof value === typeof values[0])
          ? typeof values[0] as 'string' | 'number'
          : undefined;
      return merge(
        values.length
          ? { ...(type ? { type: typeFormat(type, openApiVersion) } : {}), enum: values }
          : { not: {} },
        zodRef.description ? { description: zodRef.description } : {},
        ...schemas
      );
    }
  }
  const type = typeof Object.values(zodRef._def.values)[0] as
    | 'string'
    | 'number';
  return merge(
    {
      type: typeFormat(type, openApiVersion),
      enum: Object.values(zodRef._def.values)
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseIntersection({
  generateSchema,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodIntersection<z.ZodTypeAny, z.ZodTypeAny>>): SchemaObject {
  return merge(
    {
      allOf: [
        generateSchema(zodRef._def.left, useOutput, openApiVersion),
        generateSchema(zodRef._def.right, useOutput, openApiVersion)
      ]
    },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseUnion({
  generateSchema,
  pureInput,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodUnion<[z.ZodTypeAny, ...z.ZodTypeAny[]]>>): SchemaObject {
  const contents = zodRef._def.options;
  if (
    !pureInput &&
    contents.reduce(
      (prev, content) => prev && content._def.typeName === 'ZodLiteral',
      true
    )
  ) {
    // special case to transform unions of literals into enums
    const literals = contents as unknown as z.ZodLiteral<OpenApiZodAny>[];
    const values = literals.map((literal) => literal._def.value);
    const finiteScalars = values.every(
      (value) =>
        typeof value === 'string' ||
        typeof value === 'boolean' ||
        (typeof value === 'number' && Number.isFinite(value))
    );
    // Retain legacy handling for registrations outside the finite JSON subset.
    const type = finiteScalars
      ? values.length > 0 &&
        values.every((value) => typeof value === typeof values[0])
        ? typeof values[0]
        : null
      : literals.reduce(
          (prev, content) =>
            !prev || prev === typeof content._def.value
              ? typeof content._def.value
              : null,
          null as null | string
        );

    if (type) {
      return merge(
        {
          type: typeFormat(type as SchemaObjectType, openApiVersion),
          enum: finiteScalars ? [...new Set(values)] : values
        },
        zodRef.description ? { description: zodRef.description } : {},
        ...schemas
      );
    }
  }

  const explicitUnion = schemas.some(
    (schema) => schema.oneOf !== undefined || schema.anyOf !== undefined
  );
  if (contents.length === 0 && !explicitUnion) {
    return merge(
      { not: {} },
      zodRef.description ? { description: zodRef.description } : {},
      ...schemas
    );
  }

  const oneOfContents =
    openApiVersion === '3.0'
      ? contents.filter((content) => content._def.typeName !== 'ZodNull')
      : contents;
  const contentsHasNull = contents.length != oneOfContents.length;

  return merge(
    {
      [explicitUnion ? 'oneOf' : 'anyOf']: oneOfContents.map((schema) =>
        generateSchema(schema, useOutput, openApiVersion)
      )
    },
    contentsHasNull ? { nullable: true } : {},
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseDiscriminatedUnion({
  generateSchema,
  pureInput,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<
  z.ZodDiscriminatedUnion<string, z.ZodDiscriminatedUnionOption<string>[]>
>): SchemaObject {
  const discriminator = zodRef.discriminator;
  const oneOf = zodRef.options.map((option) => {
    const schema = generateSchema(option, useOutput, openApiVersion);
    const metadata = (option as OpenApiZodAny).metaOpenApi ?? {};
    const annotations = Array.isArray(metadata) ? metadata : [metadata];
    // Dispatch happens before defaults. Only the already-registered undefined
    // branch may receive absent tags; never execute a default to infer a tag.
    // Canonical JSON objects inherit Object.prototype; those names cannot
    // dispatch an absent own property as undefined. Null-prototype direct JS
    // inputs have distinct semantics outside this JSON representation.
    // An explicit branch required annotation remains caller-owned.
    if (
      pureInput &&
      (zodRef.optionsMap.get(undefined) !== option ||
        Object.prototype.hasOwnProperty.call(Object.prototype, discriminator)) &&
      !annotations.some((value) =>
        Object.prototype.hasOwnProperty.call(value, 'required')
      )
    ) {
      return {
        ...schema,
        required: [...new Set([...(schema.required ?? []), discriminator])]
      };
    }
    return schema;
  });
  return merge(
    { discriminator: { propertyName: discriminator }, oneOf },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseNever({
  zodRef,
  schemas
}: ParsingArgs<z.ZodNever>): SchemaObject {
  return merge(
    { not: {} },
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseBranded({
  generateSchema,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodBranded<z.ZodAny, string>>): SchemaObject {
  return merge(
    generateSchema(zodRef._def.type, useOutput, openApiVersion),
    ...schemas
  );
}

function catchAllParser({
  zodRef,
  schemas
}: ParsingArgs<ZodTypeAny>): SchemaObject {
  return merge(
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parsePipeline({
  generateSchema,
  schemas,
  zodRef,
  useOutput,
  openApiVersion
}: ParsingArgs<z.ZodPipeline<never, never>>): SchemaObject {
  return merge(
    generateSchema(
      useOutput ? zodRef._def.out : zodRef._def.in,
      useOutput,
      openApiVersion
    ),
    ...schemas
  );
}

function parseReadonly({
  generateSchema,
  zodRef,
  useOutput,
  schemas,
  openApiVersion
}: ParsingArgs<z.ZodReadonly<z.ZodAny>>): SchemaObject {
  return merge(
    generateSchema(zodRef._def.innerType, useOutput, openApiVersion),
    zodRef.description ? { description: zodRef.description } : {},
    ...schemas
  );
}

function parseTuple(
  args: ParsingArgs<
    z.ZodTuple<[z.ZodTypeAny, ...z.ZodTypeAny[]] | [], z.ZodTypeAny | null>
  >
): SchemaObject {
  if (!args.jsonSchemaDialect) return catchAllParser(args);
  const items = args.zodRef._def.items;
  const rest = args.zodRef._def.rest;
  return merge(
    {
      type: typeFormat('array', args.openApiVersion),
      minItems: items.length,
      ...(items.length
        ? {
            prefixItems: items.map((item) =>
              args.generateSchema(item, args.useOutput, args.openApiVersion)
            )
          }
        : {}),
      ...(rest
        ? {
            items: args.generateSchema(
              rest,
              args.useOutput,
              args.openApiVersion
            )
          }
        : { items: false, maxItems: items.length })
    },
    args.zodRef.description ? { description: args.zodRef.description } : {},
    ...args.schemas
  );
}

const workerMap = {
  ZodObject: parseObject,
  ZodRecord: parseRecord,
  ZodString: parseString,
  ZodNumber: parseNumber,
  ZodBigInt: parseBigInt,
  ZodBoolean: parseBoolean,
  ZodDate: parseDate,
  ZodNull: parseNull,
  ZodOptional: parseOptional,
  ZodNullable: parseNullable,
  ZodDefault: parseDefault,
  ZodArray: parseArray,
  ZodLiteral: parseLiteral,
  ZodEnum: parseEnum,
  ZodNativeEnum: parseEnum,
  ZodTransformer: parseTransformation,
  ZodEffects: parseTransformation,
  ZodIntersection: parseIntersection,
  ZodUnion: parseUnion,
  ZodDiscriminatedUnion: parseDiscriminatedUnion,
  ZodNever: parseNever,
  ZodBranded: parseBranded,
  // TODO Transform the rest to schemas
  ZodUndefined: catchAllParser,
  ZodTuple: parseTuple,
  ZodMap: catchAllParser,
  ZodFunction: catchAllParser,
  ZodLazy: catchAllParser,
  ZodPromise: catchAllParser,
  ZodAny: catchAllParser,
  ZodUnknown: catchAllParser,
  ZodVoid: catchAllParser,
  ZodPipeline: parsePipeline,
  ZodReadonly: parseReadonly
};
type WorkerKeys = keyof typeof workerMap;

export function generateSchema(
  zodRef: OpenApiZodAny,
  useOutput = false,
  openApiVersion: OpenAPIVersion = '3.1',
  options?: SchemaGenerationOptions
): SchemaObject & { $schema?: string } {
  if (
    options &&
    (options.jsonSchemaDialect !==
      'https://json-schema.org/draft/2020-12/schema' ||
      openApiVersion !== '3.1')
  ) {
    throw new Error('Explicit JSON Schema 2020-12 export requires OpenAPI 3.1');
  }
  if ((options?.pureInput || options?.zodDiagnostics) && useOutput) {
    throw new Error('Pure input export does not support output projection');
  }
  const provenance = options?.zodDiagnostics ? diagnosticProvenance(zodRef, options) : undefined;
  const context = options ? Object.freeze({ ...options, pureInput: options.pureInput || options.zodDiagnostics }) : undefined;
  let remaining = 10000;
  const active = new Set<ZodTypeAny>();
  const charge = (depth: number) => {
    if (context && (depth > 64 || --remaining < 0))
      throw new Error('Explicit JSON Schema export budget exceeded');
  };
  const generate = (
    schema: OpenApiZodAny,
    output: boolean,
    version: OpenAPIVersion,
    depth: number
  ): SchemaObject => {
    charge(depth);
    if (context?.pureInput) {
      assertPureInputSchema(schema, context.zodChecks);
      if (active.has(schema))
        throw new Error('Pure input schema cycle is unsupported');
      active.add(schema);
    }
    try {
      return render(schema, output, version, depth);
    } finally {
      active.delete(schema);
    }
  };
  const render = (
    schema: OpenApiZodAny,
    output: boolean,
    version: OpenAPIVersion,
    depth: number
  ): SchemaObject => {
    const { metaOpenApi = {} } = schema;
    const schemas = [
      ...(Array.isArray(metaOpenApi) ? metaOpenApi : [metaOpenApi])
    ];
    if (
      context &&
      schemas.some(
        (metadata) =>
          metadata.$schema !== undefined &&
          (typeof metadata.$schema !== 'string' ||
            metadata.$schema.replace(/#$/, '') !== context.jsonSchemaDialect)
      )
    ) {
      throw new Error(
        'Schema annotation dialect conflicts with explicit JSON Schema export'
      );
    }
    if (context?.zodRegex && schemas.some(metadata => Object.prototype.hasOwnProperty.call(metadata, zodRegexKeyword)))
      throw new Error('The reserved Zod regex keyword cannot be overridden');
    if (context?.zodChecks && schemas.some(metadata => Object.prototype.hasOwnProperty.call(metadata, zodChecksKeyword)))
      throw new Error('The reserved Zod check keyword cannot be overridden');
    const args = {
      zodRef: schema as never,
      schemas,
      useOutput: output,
      openApiVersion: version,
      jsonSchemaDialect: context?.jsonSchemaDialect,
      zodChecks: context?.zodChecks,
      zodRegex: context?.zodRegex,
      pureInput: context?.pureInput
        ? {
            isOptional: (child: ZodTypeAny) =>
              pureInputOptional(child, charge, depth + 1, undefined, context.zodChecks)
          }
        : undefined,
      generateSchema: (
        child: OpenApiZodAny,
        childOutput = output,
        childVersion = version
      ) => generate(child, childOutput, childVersion, depth + 1)
    };
    try {
      const typeName = schema._def.typeName as WorkerKeys;
      if (context?.pureInput && (typeName === 'ZodUndefined' || typeName === 'ZodVoid'))
        return parseNever(args as ParsingArgs<z.ZodNever>);
      if (typeName in workerMap) return workerMap[typeName](args);
      return catchAllParser(args);
    } catch (err) {
      if (context) throw err;
      console.error(err);
      return catchAllParser(args);
    }
  };
  const result = generate(zodRef, useOutput, openApiVersion, 0);
  if (!context) return result;
  const projected = { ...result, $schema: context.jsonSchemaDialect };
  if (!provenance) return projected;
  // A detached serialized binding detects later edits of assertion metadata.
  // Ordinary annotations remain unchanged; the runtime profile is opt-in.
  const projection = JSON.stringify(projected);
  if (projection.length > 16 * 1024 * 1024)
    throw new Error('Zod diagnostic projection byte budget exceeded');
  return { ...projected, [zodDiagnosticsKeyword]: { ...provenance, projection } };
}
