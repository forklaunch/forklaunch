import { stringLiteralPattern } from './stringLiteral';
import type { ZodNumberCheck, ZodStringCheck } from 'zod/v3';

export const zodChecksKeyword = 'x-forklaunch-zod-checks';
const maximumRules = 10000;
const own = (value: object, key: string) =>
  Object.prototype.hasOwnProperty.call(value, key);
type Metadata = Record<string, unknown>;
interface Checks {
  version: 1;
  arrayMinItems?: (number | 'NaN' | 'Infinity' | '-Infinity')[];
  arrayMaxItems?: (number | 'NaN' | 'Infinity' | '-Infinity')[];
  utf16MinLengths?: (number | 'NaN' | 'Infinity' | '-Infinity')[];
  utf16MaxLengths?: (number | 'NaN' | 'Infinity' | '-Infinity')[];
  utf16MinLength?: number;
  utf16MaxLength?: number;
  formats?: ('email' | 'uuid' | 'uri' | 'cuid' | 'cuid2' | 'ulid' | 'nanoid' | 'base64' | 'base64url' | 'date' | 'duration' | 'ip' | 'ipv4' | 'ipv6' | 'cidr' | 'cidrv4' | 'cidrv6' | 'emoji')[];
  jwtAlgorithms?: (string | null)[];
  dateTimes?: { precision: number | null; local: boolean; offset: boolean }[];
  timePrecisions?: (number | null)[];
  multipleOf?: (number | 'NaN' | 'Infinity' | '-Infinity')[];
}
function admit(checks: readonly unknown[], metadata: readonly Metadata[]) {
  if (checks.length > maximumRules)
    throw new Error('Zod check profile exceeds 10000 rules');
  if (metadata.some((value) => own(value, zodChecksKeyword)))
    throw new Error('The reserved Zod check keyword cannot be overridden');
}
export function projectStringChecks(
  checks: readonly ZodStringCheck[],
  metadata: readonly Metadata[],
  factoryEmail = false
): Checks | undefined {
  admit(checks, metadata);
  const formats = checks
    .filter(
      (check) =>
        ['email', 'uuid', 'url', 'cuid', 'cuid2', 'ulid', 'nanoid', 'base64', 'base64url', 'date', 'duration', 'ip', 'cidr', 'emoji'].includes(check.kind) &&
        !metadata.some((value) =>
          own(
            value,
            check.kind === 'email' && factoryEmail ? 'pattern' : 'format'
          )
        )
    )
    .map((check): NonNullable<Checks['formats']>[number] => {
      if (check.kind === 'ip' || check.kind === 'cidr') {
        if (check.version !== undefined && check.version !== 'v4' && check.version !== 'v6')
          throw new Error('Zod IP/CIDR version must be v4, v6 or undefined');
        return (check.kind + (check.version ?? '')) as NonNullable<Checks['formats']>[number];
      }
      return check.kind === 'url' ? 'uri' : (check.kind as NonNullable<Checks['formats']>[number]);
    });
  const timePrecisions = metadata.some((value) => own(value, 'format')) ? [] : checks
    .filter((check) => check.kind === 'time')
    .map((check) => {
      const precision = check.precision ?? null;
      if (precision !== null && (!Number.isInteger(precision) || precision < 0 || precision > 1000))
        throw new Error('Zod time precision must be null or an integer from 0 to 1000');
      return precision;
    });
  const dateTimes = metadata.some((value) => own(value, 'format')) ? [] : checks
    .filter((check) => check.kind === 'datetime')
    .map((check) => {
      const precision = check.precision ?? null;
      if (precision !== null && (!Number.isInteger(precision) || precision < 0 || precision > 1000))
        throw new Error('Zod datetime precision must be null or an integer from 0 to 1000');
      if (typeof check.local !== 'boolean' || typeof check.offset !== 'boolean')
        throw new Error('Zod datetime local and offset options must be booleans');
      return {precision, local: check.local, offset: check.offset};
    });
  const jwtAlgorithms = metadata.some((value) => own(value, 'format')) ? [] : checks
    .filter((check) => check.kind === 'jwt')
    .map((check) => {
      const algorithm = check.alg ?? null;
      if (algorithm !== null && typeof algorithm !== 'string')
        throw new Error('Zod JWT algorithm must be a string or null');
      return algorithm;
    });
  const formatResult: Checks | undefined = formats.length || timePrecisions.length || dateTimes.length || jwtAlgorithms.length
    ? { version: 1, ...(formats.length ? { formats } : {}), ...(timePrecisions.length ? { timePrecisions } : {}), ...(dateTimes.length ? { dateTimes } : {}), ...(jwtAlgorithms.length ? { jwtAlgorithms } : {}) }
    : undefined;
  if (
    (formats.length || timePrecisions.length || dateTimes.length || jwtAlgorithms.length) &&
    checks.some((check) =>
      ['trim', 'toLowerCase', 'toUpperCase'].includes(check.kind)
    )
  )
    throw new Error(
      'Zod check projection cannot combine string transformations with format checks'
    );
  const bounds = checks.filter(
    (check) =>
      check.kind === 'min' || check.kind === 'max' || check.kind === 'length'
  );
  if (!bounds.length) return formatResult;
  const minOverride = metadata.some((value) => own(value, 'minLength'));
  const maxOverride = metadata.some((value) => own(value, 'maxLength'));
  const needsMin = !minOverride && bounds.some((check) => check.kind !== 'max');
  const needsMax = !maxOverride && bounds.some((check) => check.kind !== 'min');
  if (!needsMin && !needsMax) return formatResult;
  if (
    checks.some(
      (check) =>
        check.kind === 'trim' ||
        check.kind === 'toLowerCase' ||
        check.kind === 'toUpperCase'
    )
  )
    throw new Error(
      'Zod check projection cannot combine string transformations with length checks'
    );
  const result: Checks = formatResult ?? { version: 1 };
  const active = bounds.filter((check) =>
    check.kind === 'length' ||
    (check.kind === 'min' ? !minOverride : !maxOverride)
  );
  if (active.some((check) => !Number.isSafeInteger(check.value) || check.value < 0)) {
    const minimumCount = minOverride ? 0 : bounds.filter(check => check.kind !== 'max').length;
    const maximumCount = maxOverride ? 0 : bounds.filter(check => check.kind !== 'min').length;
    if (minimumCount + maximumCount + formats.length + timePrecisions.length +
        dateTimes.length + jwtAlgorithms.length > maximumRules)
      throw new Error('Zod check profile exceeds 10000 rules');
    const encode = (value: number): number | 'NaN' | 'Infinity' | '-Infinity' => {
      if (typeof value !== 'number') throw new Error('Zod string length bounds must be numbers');
      return Number.isNaN(value) ? 'NaN' : value === Infinity ? 'Infinity' : value === -Infinity ? '-Infinity' : value;
    };
    if (minimumCount) result.utf16MinLengths = bounds.filter(check => check.kind !== 'max').map(check => encode(check.value));
    if (maximumCount) result.utf16MaxLengths = bounds.filter(check => check.kind !== 'min').map(check => encode(check.value));
    return result;
  }
  for (const check of bounds) {
    if (
      (check.kind === 'min' && minOverride) ||
      (check.kind === 'max' && maxOverride)
    )
      continue;
    if (!Number.isSafeInteger(check.value) || check.value < 0)
      throw new Error(
        'Zod check length bounds must be nonnegative safe integers'
      );
    if (!minOverride && (check.kind === 'min' || check.kind === 'length'))
      result.utf16MinLength = Math.max(result.utf16MinLength ?? 0, check.value);
    if (!maxOverride && (check.kind === 'max' || check.kind === 'length'))
      result.utf16MaxLength = Math.min(
        result.utf16MaxLength ?? check.value,
        check.value
      );
  }
  return Object.keys(result).length > 1 ? result : undefined;
}
export function projectNumberChecks(
  checks: readonly ZodNumberCheck[],
  metadata: readonly Metadata[]
): Checks | undefined {
  admit(checks, metadata);
  if (metadata.some((value) => own(value, 'multipleOf'))) return undefined;
  const divisors = checks
    .filter((check) => check.kind === 'multipleOf')
    .map((check) =>
      Number.isNaN(check.value)
        ? ('NaN' as const)
        : check.value === Infinity
          ? ('Infinity' as const)
          : check.value === -Infinity
            ? ('-Infinity' as const)
            : check.value
    );
  return divisors.length ? { version: 1, multipleOf: divisors } : undefined;
}

/** Retained array checks are independent conjunctions, not ordered transforms. */
export function projectArrayChecks(
  definition: { exactLength: {value:number} | null; minLength: {value:number} | null; maxLength: {value:number} | null },
  metadata: readonly Metadata[]
): Checks | undefined {
  admit([], metadata);
  const encode = (value: number): number | 'NaN' | 'Infinity' | '-Infinity' => {
    if (typeof value !== 'number') throw new Error('Zod array cardinality must be numeric');
    return Number.isNaN(value) ? 'NaN' : value === Infinity ? 'Infinity' : value === -Infinity ? '-Infinity' : value;
  };
  const minimum = metadata.some(value=>own(value,'minItems')) ? [] : [definition.exactLength,definition.minLength].filter(value=>value!==null).map(value=>encode(value.value));
  const maximum = metadata.some(value=>own(value,'maxItems')) ? [] : [definition.exactLength,definition.maxLength].filter(value=>value!==null).map(value=>encode(value.value));
  return minimum.length || maximum.length ? {version:1,...(minimum.length?{arrayMinItems:minimum}:{}),...(maximum.length?{arrayMaxItems:maximum}:{})} : undefined;
}

/** Missing portable patterns retain literal code units, never regex semantics.
 * The generated allOf location deliberately preserves existing annotation merge.
 */
export function projectStringPredicates(checks: readonly ZodStringCheck[], metadata: readonly Metadata[], otherRules = 0) {
  admit(checks, metadata);
  const rules = checks.filter(check =>
    ['startsWith','endsWith','includes'].includes(check.kind) && stringLiteralPattern(check) === undefined
  );
  if (!rules.length) return undefined;
  if (rules.length + otherRules > maximumRules) throw new Error('Zod check profile exceeds 10000 rules');
  if (checks.some(check=>['trim','toLowerCase','toUpperCase'].includes(check.kind)))
    throw new Error('Zod check projection cannot combine string transformations with literal predicates');
  let remaining = 262144;
  const stringPredicates = rules.map(check => {
    if (check.kind !== 'startsWith' && check.kind !== 'endsWith' && check.kind !== 'includes')
      throw new Error('Invalid string predicate');
    if (typeof check.value !== 'string' || check.value.length > 4096 || (remaining -= check.value.length) < 0)
      throw new Error('Zod string predicate source unit limit');
    const position = check.kind === 'includes' && check.position !== undefined ? check.position : 0;
    if (typeof position !== 'number') throw new Error('Zod string predicate position must be numeric');
    return {kind:check.kind,sourceUnits:Array.from({length:check.value.length},(_,index)=>check.value.charCodeAt(index)),
      ...(check.kind === 'includes' ? {position:Number.isNaN(position)?'NaN':position===Infinity?'Infinity':position===-Infinity?'-Infinity':position} : {})};
  });
  return {version:1 as const,stringPredicates};
}
