import type Ajv from 'ajv';
// BEGIN GENERATED UNICODE16 EMOJI RANGES
const zodEmojiRanges: readonly (readonly [number, number])[] = [
  [35, 35],
  [42, 42],
  [48, 57],
  [169, 169],
  [174, 174],
  [8205, 8205],
  [8252, 8252],
  [8265, 8265],
  [8419, 8419],
  [8482, 8482],
  [8505, 8505],
  [8596, 8601],
  [8617, 8618],
  [8986, 8987],
  [9000, 9000],
  [9096, 9096],
  [9167, 9167],
  [9193, 9203],
  [9208, 9210],
  [9410, 9410],
  [9642, 9643],
  [9654, 9654],
  [9664, 9664],
  [9723, 9726],
  [9728, 9733],
  [9735, 9746],
  [9748, 9861],
  [9872, 9989],
  [9992, 10002],
  [10004, 10004],
  [10006, 10006],
  [10013, 10013],
  [10017, 10017],
  [10024, 10024],
  [10035, 10036],
  [10052, 10052],
  [10055, 10055],
  [10060, 10060],
  [10062, 10062],
  [10067, 10069],
  [10071, 10071],
  [10083, 10087],
  [10133, 10135],
  [10145, 10145],
  [10160, 10160],
  [10175, 10175],
  [10548, 10549],
  [11013, 11015],
  [11035, 11036],
  [11088, 11088],
  [11093, 11093],
  [12336, 12336],
  [12349, 12349],
  [12951, 12951],
  [12953, 12953],
  [65039, 65039],
  [126976, 127231],
  [127245, 127247],
  [127279, 127279],
  [127340, 127345],
  [127358, 127359],
  [127374, 127374],
  [127377, 127386],
  [127405, 127487],
  [127489, 127503],
  [127514, 127514],
  [127535, 127535],
  [127538, 127546],
  [127548, 127551],
  [127561, 128317],
  [128326, 128591],
  [128640, 128767],
  [128884, 128895],
  [128981, 129023],
  [129036, 129039],
  [129096, 129103],
  [129114, 129119],
  [129160, 129167],
  [129198, 129279],
  [129292, 129338],
  [129340, 129349],
  [129351, 129791],
  [130048, 131069],
  [917536, 917631],
];
// END GENERATED UNICODE16 EMOJI RANGES
function emoji(value: string): boolean {
  if (!value.length) return false;
  for (const ch of value) {
    const code = ch.codePointAt(0)!;
    let low = 0, high = zodEmojiRanges.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (zodEmojiRanges[middle][1] < code) low = middle + 1;
      else high = middle;
    }
    if (low === zodEmojiRanges.length || zodEmojiRanges[low][0] > code) return false;
  }
  return true;
}

// Reserved versioned capability; arbitrary JSON Schema keywords are unchanged.
const zodChecksKeyword = 'x-forklaunch-zod-checks';
interface ZodChecks {
  version: 1;
  stringPredicates?: {kind: 'startsWith' | 'endsWith' | 'includes'; sourceUnits: number[]; position?: number | 'NaN' | 'Infinity' | '-Infinity'}[];
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
function compileZodChecks(input: unknown): (value: unknown) => boolean {
  if (!input || typeof input !== 'object' || Array.isArray(input))
    throw new Error('Invalid Zod check profile');
  const value = input as Record<string, unknown>;
  if (
    value.version !== 1 ||
    Object.keys(value).some(
      (key) =>
        ![
          'version',
          'stringPredicates',
          'utf16MinLengths',
          'utf16MaxLengths',
          'utf16MinLength',
          'utf16MaxLength',
          'multipleOf',
          'arrayMinItems',
          'arrayMaxItems',
          'formats',
          'timePrecisions',
          'dateTimes',
          'jwtAlgorithms'
        ].includes(key)
    )
  )
    throw new Error('Invalid Zod check profile version or fields');
  for (const key of ['utf16MinLength', 'utf16MaxLength']) {
    if (
      Object.prototype.hasOwnProperty.call(value, key) &&
      (typeof value[key] !== 'number' ||
        !Number.isSafeInteger(value[key]) ||
        (value[key] as number) < 0)
    )
      throw new Error('Invalid Zod UTF-16 bound');
  }
  for (const key of ['multipleOf', 'arrayMinItems', 'arrayMaxItems', 'utf16MinLengths', 'utf16MaxLengths']) {
    if (Object.prototype.hasOwnProperty.call(value,key)) {
      const rules=value[key];
      if (!Array.isArray(rules) || rules.length>10000 || rules.some(rule=>
        !(typeof rule==='number' && Number.isFinite(rule)) && !['NaN','Infinity','-Infinity'].includes(rule)))
        throw new Error('Invalid Zod numeric rules');
    }
  }
  if (
    Object.prototype.hasOwnProperty.call(value, 'formats') &&
    (!Array.isArray(value.formats) ||
      value.formats.length > 10000 ||
      value.formats.some(
        (format) => !['email', 'uuid', 'uri', 'cuid', 'cuid2', 'ulid', 'nanoid', 'base64', 'base64url', 'date', 'duration', 'ip', 'ipv4', 'ipv6', 'cidr', 'cidrv4', 'cidrv6', 'emoji'].includes(format)
      ))
  )
    throw new Error('Invalid Zod format rules');
  if (Object.prototype.hasOwnProperty.call(value, 'timePrecisions') &&
      (!Array.isArray(value.timePrecisions) || value.timePrecisions.length > 10000 ||
       value.timePrecisions.some((p) => p !== null && (typeof p !== 'number' || !Number.isInteger(p) || p < 0 || p > 1000))))
    throw new Error('Invalid Zod time precisions');
  if (Object.prototype.hasOwnProperty.call(value, 'dateTimes') &&
      (!Array.isArray(value.dateTimes) || value.dateTimes.length > 10000 || value.dateTimes.some((rule) =>
        !rule || typeof rule !== 'object' || Array.isArray(rule) || Object.keys(rule).length !== 3 ||
        typeof rule.local !== 'boolean' || typeof rule.offset !== 'boolean' ||
        (rule.precision !== null && (typeof rule.precision !== 'number' || !Number.isInteger(rule.precision) || rule.precision < 0 || rule.precision > 1000)))))
    throw new Error('Invalid Zod datetime rules');
  if (Object.prototype.hasOwnProperty.call(value, 'jwtAlgorithms') &&
      (!Array.isArray(value.jwtAlgorithms) || value.jwtAlgorithms.length > 10000 ||
       value.jwtAlgorithms.some((algorithm) => algorithm !== null && typeof algorithm !== 'string')))
    throw new Error('Invalid Zod JWT algorithms');
  if (Object.prototype.hasOwnProperty.call(value, 'stringPredicates')) {
    const rules = value.stringPredicates;
    if (!Array.isArray(rules) || rules.length > 10000) throw new Error('Invalid string predicates');
    let remaining = 262144;
    for (const rule of rules) {
      if (!rule || typeof rule !== 'object' || Array.isArray(rule) ||
          !['startsWith','endsWith','includes'].includes(rule.kind) ||
          Object.keys(rule).length !== (rule.kind === 'includes' ? 3 : 2) ||
          !Object.prototype.hasOwnProperty.call(rule,'kind') || !Object.prototype.hasOwnProperty.call(rule,'sourceUnits') ||
          Object.keys(rule).some(key=>!['kind','sourceUnits',...(rule.kind==='includes'?['position']:[])].includes(key)) ||
          !Array.isArray(rule.sourceUnits) || rule.sourceUnits.length > 4096 ||
          (remaining -= rule.sourceUnits.length) < 0 ||
          rule.sourceUnits.some((unit: unknown)=>typeof unit !== 'number' || !Number.isInteger(unit) || unit < 0 || unit > 65535) ||
          (rule.kind === 'includes' && !(typeof rule.position === 'number' && Number.isFinite(rule.position)) && !['NaN','Infinity','-Infinity'].includes(rule.position)))
        throw new Error('Invalid string predicate rule');
    }
  }
  if (
    (Array.isArray(value.stringPredicates) ? value.stringPredicates.length : 0) +
    (Array.isArray(value.utf16MinLengths) ? value.utf16MinLengths.length : 0) +
    (Array.isArray(value.utf16MaxLengths) ? value.utf16MaxLengths.length : 0) +
    (Array.isArray(value.arrayMinItems) ? value.arrayMinItems.length : 0) +
    (Array.isArray(value.arrayMaxItems) ? value.arrayMaxItems.length : 0) +
    (Array.isArray(value.jwtAlgorithms) ? value.jwtAlgorithms.length : 0) +
    (Array.isArray(value.dateTimes) ? value.dateTimes.length : 0) +
    (Array.isArray(value.timePrecisions) ? value.timePrecisions.length : 0) +
    (Array.isArray(value.formats) ? value.formats.length : 0) +
      (Array.isArray(value.multipleOf) ? value.multipleOf.length : 0) >
    10000
  )
    throw new Error('Zod check profile exceeds 10000 rules');
  const profile = value as unknown as ZodChecks;
  const predicates = profile.stringPredicates?.map(rule=>({kind:rule.kind,
    needle:rule.sourceUnits.map(unit=>String.fromCharCode(unit)).join(''),
    position:typeof rule.position === 'number' ? rule.position : Number(rule.position)}));
  // Capture immutable admitted configuration; do not retain mutable user arrays.
  const minimum = profile.utf16MinLength,
    maximum = profile.utf16MaxLength;
  const divisors = profile.multipleOf?.map((divisor) =>
    typeof divisor === 'number' ? divisor : Number(divisor)
  );
  const utf16Minimum = profile.utf16MinLengths?.map(bound=>typeof bound==='number'?bound:Number(bound));
  const utf16Maximum = profile.utf16MaxLengths?.map(bound=>typeof bound==='number'?bound:Number(bound));
  const arrayMinimum = profile.arrayMinItems?.map(bound=>typeof bound==='number'?bound:Number(bound));
  const arrayMaximum = profile.arrayMaxItems?.map(bound=>typeof bound==='number'?bound:Number(bound));
  const formatChecks = profile.formats?.slice();
  const jwtAlgorithms = profile.jwtAlgorithms?.slice();
  const timeSource = (p: number | null) =>
    `([01]\\d|2[0-3]):[0-5]\\d(:[0-5]\\d${p ? `\\.\\d{${p}}` : p === null ? '(\\.\\d+)?' : ''})${p ? '+' : '?'}`;
  const timeChecks = profile.timePrecisions?.map((p) => new RegExp(`^${timeSource(p)}$`));
  const email =
    /^(?!\.)(?!.*\.\.)([A-Z0-9_'+\-\.]*)[A-Z0-9_+-]@([A-Z0-9][A-Z0-9\-]*\.)+[A-Z]{2,}$/i;
  const uuid =
    /^[0-9a-fA-F]{8}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{12}$/i;
  const simpleFormats = {
    ipv4: /^(?:(?:25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])\.){3}(?:25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])$/,
    ipv6: /^(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))$/,
    cidrv4: /^(?:(?:25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])\.){3}(?:25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])\/(3[0-2]|[12]?[0-9])$/,
    cidrv6: /^(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))\/(12[0-8]|1[01][0-9]|[1-9]?[0-9])$/,
    duration: /^[-+]?P(?!$)(?:(?:[-+]?\d+Y)|(?:[-+]?\d+[.,]\d+Y$))?(?:(?:[-+]?\d+M)|(?:[-+]?\d+[.,]\d+M$))?(?:(?:[-+]?\d+W)|(?:[-+]?\d+[.,]\d+W$))?(?:(?:[-+]?\d+D)|(?:[-+]?\d+[.,]\d+D$))?(?:T(?=[\d+-])(?:(?:[-+]?\d+H)|(?:[-+]?\d+[.,]\d+H$))?(?:(?:[-+]?\d+M)|(?:[-+]?\d+[.,]\d+M$))?(?:[-+]?\d+(?:[.,]\d+)?S)?)??$/,
    date: /^((\d\d[2468][048]|\d\d[13579][26]|\d\d0[48]|[02468][048]00|[13579][26]00)-02-29|\d{4}-((0[13578]|1[02])-(0[1-9]|[12]\d|3[01])|(0[469]|11)-(0[1-9]|[12]\d|30)|(02)-(0[1-9]|1\d|2[0-8])))$/,
    cuid: /^c[^\s-]{8,}$/i,
    cuid2: /^[0-9a-z]+$/,
    ulid: /^[0-9A-HJKMNP-TV-Z]{26}$/i,
    nanoid: /^[a-z0-9_-]{21}$/i,
    base64: /^([0-9a-zA-Z+/]{4})*(([0-9a-zA-Z+/]{2}==)|([0-9a-zA-Z+/]{3}=))?$/,
    base64url: /^([0-9a-zA-Z-_]{4})*(([0-9a-zA-Z-_]{2}(==)?)|([0-9a-zA-Z-_]{3}(=)?))?$/
  };
  const dateTimeChecks = profile.dateTimes?.map((rule) => new RegExp(
    `^${simpleFormats.date.source.slice(1,-1)}T${timeSource(rule.precision)}(${rule.local ? 'Z?' : 'Z'}${rule.offset ? '|([+-]\\d{2}:?\\d{2})' : ''})$`
  ));
  return (data: unknown) => {
    if (typeof data === 'string' && predicates && !predicates.every(rule=>
      rule.kind === 'includes' ? data.includes(rule.needle,rule.position) :
      rule.kind === 'startsWith' ? data.startsWith(rule.needle) : data.endsWith(rule.needle))) return false;
    if (typeof data === 'string' && jwtAlgorithms && !jwtAlgorithms.every((algorithm) => jwt(data, algorithm))) return false;
    if (typeof data === 'string' && dateTimeChecks && !dateTimeChecks.every((check) => check.test(data))) return false;
    if (typeof data === 'string' && timeChecks && !timeChecks.every((check) => check.test(data))) return false;
    if (typeof data === 'string' && formatChecks)
      for (const format of formatChecks) {
        if (format === 'emoji' && !emoji(data)) return false;
        if (format === 'ip' && !simpleFormats.ipv4.test(data) && !simpleFormats.ipv6.test(data)) return false;
        if (format === 'cidr' && !simpleFormats.cidrv4.test(data) && !simpleFormats.cidrv6.test(data)) return false;
        if (format in simpleFormats && !simpleFormats[format as keyof typeof simpleFormats].test(data)) return false;
        if (format === 'email' && !email.test(data)) return false;
        if (format === 'uuid' && !uuid.test(data)) return false;
        if (format === 'uri') {
          try {
            new URL(data);
          } catch {
            return false;
          }
        }
      }

    if (
      typeof data === 'string' &&
      ((utf16Minimum?.some(bound=>data.length<bound)) ||
        (utf16Maximum?.some(bound=>data.length>bound)) ||
        (minimum !== undefined && data.length < minimum) ||
        (maximum !== undefined && data.length > maximum))
    )
      return false;
    if (Array.isArray(data) && (arrayMinimum?.some(bound=>data.length<bound) || arrayMaximum?.some(bound=>data.length>bound))) return false;
    if (typeof data === 'number' && divisors) {
      for (const divisor of divisors) {
        const decimals = Math.max(
          (data.toString().split('.')[1] || '').length,
          (divisor.toString().split('.')[1] || '').length
        );
        const numerator = parseInt(data.toFixed(decimals).replace('.', ''));
        const denominator = parseInt(
          divisor.toFixed(decimals).replace('.', '')
        );
        if ((numerator % denominator) / Math.pow(10, decimals) !== 0)
          return false;
      }
    }
    return true;
  };
}

/** Add the reserved capability without replacing the supplied engine/options.
 * Reusing an engine with this module instance is supported. Separately bundled
 * consumers must use separate engines; their handler identities differ. A caller-installed handler under the reserved
 * name is rejected rather than silently trusting different validation semantics.
 */
export function registerZodChecks(ajv: Ajv): Ajv {
  const existing = ajv.getKeyword(zodChecksKeyword);
  if (existing) {
    if (
      typeof existing === 'object' &&
      'compile' in existing &&
      existing.compile === compileZodChecks
    )
      return ajv;
    throw new Error('Reserved Zod check keyword is already registered');
  }
  ajv.addKeyword({
    keyword: zodChecksKeyword,
    schemaType: 'object',
    compile: compileZodChecks,
    errors: false
  });
  return ajv;
}

// Raw Zod syntax/header policy only; this is not JWT authentication.
function jwt(value: string, algorithm: string | null): boolean {
  if (!/^[A-Za-z0-9-_]+\.[A-Za-z0-9-_]+\.[A-Za-z0-9-_]*$/.test(value)) return false;
  try {
    const header = value.split('.')[0];
    const base64 = header.replace(/-/g, '+').replace(/_/g, '/')
      .padEnd(header.length + ((4 - (header.length % 4)) % 4), '=');
    const decoded = JSON.parse(atob(base64));
    if (typeof decoded !== 'object' || decoded === null) return false;
    if ('typ' in decoded && decoded.typ !== 'JWT') return false;
    return !!decoded.alg && (!algorithm || decoded.alg === algorithm);
  } catch {
    return false;
  }
}
