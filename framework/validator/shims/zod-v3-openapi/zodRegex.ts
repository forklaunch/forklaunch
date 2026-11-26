export const zodRegexKeyword = 'x-forklaunch-zod-regex';
type Check = { kind: string; regex?: RegExp };
const own = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);
/** Source-pattern units preserve lone surrogates through JSON serialization. */
export function projectRegexChecks(checks: readonly Check[], metadata: readonly Record<string, unknown>[]) {
  if (metadata.some(value => own(value, zodRegexKeyword)))
    throw new Error('Reserved Zod regex metadata cannot be overridden');
  // Explicit standard pattern annotations deliberately replace source regex checks.
  if (metadata.some(value => own(value, 'pattern'))) return undefined;
  const regexes = checks.filter(check => check.kind === 'regex');
  if (!regexes.length) return undefined;
  if (checks.some(check => ['trim','toLowerCase','toUpperCase'].includes(check.kind)))
    throw new Error('Zod regex profile does not project ordered string transforms');
  if (regexes.length > 64) throw new Error('Zod regex rule limit');
  let remaining = 64 * 4096;
  const rules = regexes.map(check => {
    const regex = check.regex;
    if (!(regex instanceof RegExp)) throw new Error('Invalid Zod regex check');
    const source = regex.source;
    if (source.length > 4096 || (remaining -= source.length) < 0)
      throw new Error('Zod regex source limit');
    return {sourceUnits: Array.from({length:source.length},(_,index)=>source.charCodeAt(index)), flags:regex.flags};
  });
  return {version:1 as const,rules};
}
