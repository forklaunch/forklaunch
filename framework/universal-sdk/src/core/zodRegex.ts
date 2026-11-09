import type Ajv from 'ajv';

/** Explicit host RegExp execution: synchronous and not interruptible/budgeted. */
export type ZodRegexExecution = 'host-native';
const keyword = 'x-forklaunch-zod-regex';
const maximumRules = 64;
const maximumSourceUnits = 4096;

function compileHost(schema: unknown): (value: string) => boolean {
  if (!schema || typeof schema !== 'object' || Array.isArray(schema))
    throw new Error('Invalid Zod regex profile');
  const profile = schema as Record<string, unknown>;
  if (Object.keys(profile).length !== 2 || profile.version !== 1 || !Array.isArray(profile.rules) || profile.rules.length > maximumRules)
    throw new Error('Invalid Zod regex profile');
  let remaining = maximumRules * maximumSourceUnits;
  const expressions: RegExp[] = [];
  for (const candidate of profile.rules) {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate))
      throw new Error('Invalid Zod regex rule');
    const rule = candidate as Record<string, unknown>;
    if (Object.keys(rule).length !== 2 || !Array.isArray(rule.sourceUnits) || rule.sourceUnits.length > maximumSourceUnits || typeof rule.flags !== 'string' || rule.flags.length > 8)
      throw new Error('Invalid Zod regex rule');
    remaining -= rule.sourceUnits.length;
    if (remaining < 0 || !/^[dgimsuvy]*$/.test(rule.flags) || new Set(rule.flags).size !== rule.flags.length || (rule.flags.includes('u') && rule.flags.includes('v')))
      throw new Error('Invalid Zod regex limits or flags');
    // All units are admitted before allocating the source string.
    for (const unit of rule.sourceUnits)
      if (!Number.isInteger(unit) || unit < 0 || unit > 65535)
        throw new Error('Invalid Zod regex source unit');
    const source = String.fromCharCode(...rule.sourceUnits);
    expressions.push(new RegExp(source, rule.flags));
  }
  return (value: string) => {
    for (const expression of expressions) {
      expression.lastIndex = 0;
      if (!expression.test(value)) return false;
    }
    return true;
  };
}
function disabled(): never {
  throw new Error('x-forklaunch-zod-regex requires explicit host-native regex execution policy');
}

/** Register the reserved keyword even when disabled, so strict:false cannot ignore it.
 * Caller-installed conflicting keyword handlers are never overwritten. Sharing
 * engines with separately emitted helper implementations is rejected explicitly.
 */
export function registerZodRegex(ajv: Ajv, execution?: ZodRegexExecution): Ajv {
  if (execution !== undefined && execution !== 'host-native')
    throw new Error('Unsupported Zod regex execution policy');
  const compile = execution === 'host-native' ? compileHost : disabled;
  const existing = ajv.getKeyword(keyword);
  if (existing) {
    if (typeof existing !== 'object' || !('compile' in existing) || existing.compile !== compile)
      throw new Error('Conflicting reserved Zod regex capability');
    return ajv;
  }
  ajv.addKeyword({keyword, type: 'string', schemaType: 'object', compile});
  return ajv;
}
