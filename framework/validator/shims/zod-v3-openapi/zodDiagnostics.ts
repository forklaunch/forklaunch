import type { ZodTypeAny } from 'zod/v3';

export const zodDiagnosticsKeyword = 'x-forklaunch-zod-diagnostics';
/** Identity-bound declarations, never evaluations of Zod default closures. */
export interface ZodDiagnosticOptions {
  readonly diagnosticDefaults?: ReadonlyMap<unknown, unknown>;
}
type Node = Record<string, unknown>;
const annotations = new Set(['title', 'description', 'default', 'examples', 'example', 'deprecated', 'readOnly', 'writeOnly', '$comment']);
const own = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);
const numeric = (value: number): number | string => Number.isNaN(value) ? 'NaN' : value === Infinity ? 'Infinity' : value === -Infinity ? '-Infinity' : value;

/** Retain source kinds and order, rather than deriving Zod errors from JSON Schema.
 * This operates on ordinary constructed schemas; arbitrary getters/shape thunks
 * are not sandboxed. Effects and error-map callbacks require separate bindings.
 */
export function diagnosticProvenance(root: ZodTypeAny, options: ZodDiagnosticOptions = {}): {version: 1; schema: Node} {
  let nodes = 10000;
  let bytes = 16 * 1024 * 1024;
  const active = new Set<ZodTypeAny>();
  const charge = (depth: number) => {
    if (depth > 64 || --nodes < 0) throw new Error('Zod diagnostic provenance node/depth budget exceeded');
  };
  const text = (value: string): string => {
    // A conservative JSON-escaped storage charge precedes output allocation.
    bytes -= 6 * value.length + 32;
    if (bytes < 0) throw new Error('Zod diagnostic provenance byte budget exceeded');
    for (let index = 0; index < value.length; index++) {
      const unit = value.charCodeAt(index);
      if (unit >= 0xd800 && unit <= 0xdbff) {
        const low = value.charCodeAt(++index);
        if (!(low >= 0xdc00 && low <= 0xdfff)) throw new Error('Zod diagnostics require representable UTF-8 strings');
      } else if (unit >= 0xdc00 && unit <= 0xdfff) throw new Error('Zod diagnostics require representable UTF-8 strings');
    }
    return value;
  };
  const list = <T, U>(items: readonly T[], project: (value: T) => U): U[] => {
    if (items.length > nodes || (bytes -= items.length * 32) < 0)
      throw new Error('Zod diagnostic collection budget exceeded');
    return items.map(project);
  };
  const keys = (value: object): string[] => {
    if (![Object.prototype, null].includes(Object.getPrototypeOf(value)))
      throw new Error('Zod diagnostics require plain own-key objects');
    const result: string[] = [];
    for (const key in value) if (own(value, key)) {
      if (result.length >= nodes || (bytes -= 32) < 0)
        throw new Error('Zod diagnostic collection budget exceeded');
      result.push(key);
    }
    return result;
  };
  const json = (value: unknown, depth: number): unknown => {
    charge(depth);
    if (value === null || typeof value === 'boolean') return value;
    if (typeof value === 'string') return text(value);
    if (typeof value === 'number' && Number.isFinite(value)) {
      if (Object.is(value, -0)) throw new Error('Zod diagnostic defaults cannot serialize negative zero without losing identity');
      return value;
    }
    if (Array.isArray(value)) return list(value, item => json(item, depth + 1));
    if (value && typeof value === 'object' && [Object.prototype, null].includes(Object.getPrototypeOf(value))) {
      const output: Record<string, unknown> = {};
      for (const key of keys(value)) Object.defineProperty(output, text(key), {value: json((value as Record<string, unknown>)[key], depth + 1), enumerable: true});
      return output;
    }
    throw new Error('Zod diagnostic defaults must be finite JSON values');
  };
  const scalar = (value: unknown): Node => {
    if (value === undefined) return {kind: 'undefined'};
    if (value === null) return {kind: 'null'};
    if (typeof value === 'string') return {kind: 'string', value: text(value)};
    if (typeof value === 'boolean') return {kind: 'boolean', value};
    if (typeof value === 'number') return {kind: 'number', value: numeric(value)};
    throw new Error('Unsupported Zod diagnostic literal');
  };
  const visit = (schema: ZodTypeAny, depth: number): Node => {
    charge(depth);
    if (active.has(schema)) throw new Error('Cyclic Zod diagnostic provenance is unsupported');
    active.add(schema);
    try {
      const metadata = (schema as ZodTypeAny & {metaOpenApi?: Node | Node[]}).metaOpenApi;
      for (const annotation of metadata === undefined ? [] : Array.isArray(metadata) ? metadata : [metadata]) {
        for (const key of keys(annotation)) {
          if (key === zodDiagnosticsKeyword || !annotations.has(key)) throw new Error('Zod diagnostic provenance conflicts with validation annotation: ' + key);
        }
      }
      const def = schema._def;
      if (def.errorMap !== undefined && !['ZodOptional', 'ZodNullable', 'ZodDefault', 'ZodReadonly', 'ZodBranded'].includes(def.typeName)) throw new Error('Zod diagnostic error-map callbacks require an explicit runtime binding');
      if (def.coerce) throw new Error('Built-in Zod coercion is outside the diagnostic provenance profile');
      const child = (value: ZodTypeAny) => visit(value, depth + 1);
      switch (def.typeName) {
        case 'ZodOptional': return {kind: 'optional', schema: child(def.innerType)};
        case 'ZodNullable': return {kind: 'nullable', schema: child(def.innerType)};
        case 'ZodDefault': {
          if (!options.diagnosticDefaults?.has(schema)) throw new Error('Zod default diagnostic provenance requires an identity-bound declared value');
          return {kind: 'default', schema: child(def.innerType), value: json(options.diagnosticDefaults.get(schema), depth + 1)};
        }
        case 'ZodReadonly': return child(def.innerType);
        case 'ZodBranded': return child(def.type);
        case 'ZodAny': case 'ZodUnknown': return {kind: 'unknown'};
        case 'ZodNull': return {kind: 'null'};
        case 'ZodUndefined': return {kind: 'undefined'};
        case 'ZodVoid': return {kind: 'void'};
        case 'ZodNever': return {kind: 'never'};
        case 'ZodBoolean': return {kind: 'strictBoolean'};
        case 'ZodLiteral': return {kind: 'literal', literal: scalar(def.value)};
        case 'ZodEnum': return {kind: 'enum', values: list(def.values as string[], value => text(value))};
        case 'ZodNativeEnum': return {kind: 'nativeEnum', entries: list(keys(def.values), key => ({name: text(key), literal: scalar(def.values[key])}))};
        case 'ZodString': case 'ZodNumber': {
          const checks = list(def.checks as Record<string, unknown>[], (check) => {
            charge(depth + 1);
            const output: Node = {};
            for (const key of keys(check)) {
              const value = check[key];
              if (value === undefined) continue;
              if (value instanceof RegExp) {
                if (value.source.length > nodes || (bytes -= value.source.length * 8) < 0) throw new Error('Zod diagnostic regex source budget exceeded');
                output[key] = {sourceUnits: Array.from({length: value.source.length}, (_, index) => value.source.charCodeAt(index)), flags: value.flags};
              }
              else output[key] = typeof value === 'number' ? numeric(value) : typeof value === 'string' ? text(value) : value;
            }
            return output;
          });
          return {kind: def.typeName === 'ZodString' ? 'string' : 'strictNumber', checks};
        }
        case 'ZodArray': return {kind: 'array', element: child(def.type), checks: ['exactLength', 'minLength', 'maxLength'].flatMap(key => {
          const check = def[key];
          return check === null || check === undefined ? [] : [{kind: key, value: numeric(check.value), ...(check.message === undefined ? {} : {message: text(check.message)})}];
        })};
        case 'ZodTuple': return {kind: 'tuple', items: list(def.items as ZodTypeAny[], child), rest: def.rest ? child(def.rest) : null};
        case 'ZodObject': {
          const shape = def.shape();
          return {kind: 'object', fields: list(keys(shape), name => ({name: text(name), schema: child(shape[name])})), unknown: def.unknownKeys, catchall: child(def.catchall)};
        }
        case 'ZodUnion': return {kind: 'union', options: list(def.options as ZodTypeAny[], child)};
        case 'ZodDiscriminatedUnion': return {kind: 'discriminatedUnion', discriminator: text(def.discriminator), options: list(def.options as ZodTypeAny[], child)};
        case 'ZodIntersection': return {kind: 'intersection', left: child(def.left), right: child(def.right)};
        case 'ZodRecord': return {kind: 'record', key: child(def.keyType), value: child(def.valueType)};
        default: throw new Error('Unsupported Zod diagnostic provenance node: ' + def.typeName);
      }
    } finally { active.delete(schema); }
  };
  return {version: 1, schema: visit(root, 0)};
}
