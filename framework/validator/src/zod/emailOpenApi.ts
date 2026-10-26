import { z } from 'zod/v3';
import { stringLiteralPattern } from '../../shims/zod-v3-openapi/stringLiteral';

// Kept on the definition so ordinary wrapper/export cloning preserves identity
// without inspecting titles, user annotations, or executing schema callbacks.
const factoryEmail = Symbol('ForkLaunch Zod email input grammar');

export const factoryEmailPattern =
  "^(?!\\.)(?!.*\\.\\.)([A-Za-z0-9_'+\\-\\.]*)[A-Za-z0-9_+-]@([A-Za-z0-9][A-Za-z0-9\\-]*\\.)+[A-Za-z]{2,}$";

export function markFactoryEmail(schema: z.ZodString): z.ZodString {
  const definition = { ...schema._def, [factoryEmail]: true };
  return new z.ZodString(definition);
}

export function hasFactoryEmailGrammar(schema: z.ZodString): boolean {
  const definition = schema._def as z.ZodStringDef & {
    [factoryEmail]?: boolean;
  };
  return (
    definition[factoryEmail] === true &&
    definition.checks.some((check) => check.kind === 'email') &&
    definition.checks.every(
      (check) =>
        check.kind === 'email' ||
        check.kind === 'min' ||
        check.kind === 'max' ||
        check.kind === 'length' ||
        stringLiteralPattern(check) !== undefined
    )
  );
}
