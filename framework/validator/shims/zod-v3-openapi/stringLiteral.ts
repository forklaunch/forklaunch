import type { ZodStringCheck } from 'zod/v3';

/** A literal pattern only: arbitrary regex semantics and UTF-16 offsets are not
 * translated. JSON Schema consumers may use Unicode regex mode, so a registration
 * containing an isolated surrogate cannot faithfully denote a code-unit fragment.
 */
export function stringLiteralPattern(
  check: ZodStringCheck
): string | undefined {
  if (
    check.kind !== 'startsWith' &&
    check.kind !== 'endsWith' &&
    check.kind !== 'includes'
  )
    return undefined;
  for (const character of check.value) {
    const point = character.codePointAt(0)!;
    if (point >= 0xd800 && point <= 0xdfff) return undefined;
  }
  if (
    check.kind === 'includes' &&
    check.value !== '' &&
    check.position !== undefined &&
    check.position !== 0
  )
    return undefined;
  const literal = check.value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  if (check.kind === 'startsWith') return `^(?:${literal})`;
  // $ also matches before a final newline in ECMAScript; endsWith does not.
  if (check.kind === 'endsWith') return `(?:${literal})(?![\\s\\S])`;
  return `(?:${literal})`;
}
