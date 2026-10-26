import { z } from 'zod/v3';
import { stringLiteralPattern } from '../../shims/zod-v3-openapi/stringLiteral';

const factoryUri = Symbol('ForkLaunch Zod WHATWG URL input grammar');
export const factoryUriFormat = 'forklaunch-whatwg-url';

export function markFactoryUri(schema: z.ZodString): z.ZodString {
  const definition = { ...schema._def, [factoryUri]: true };
  return new z.ZodString(definition);
}

export function hasFactoryUriGrammar(schema: z.ZodString): boolean {
  const definition = schema._def as z.ZodStringDef & {
    [factoryUri]?: boolean;
  };
  return (
    definition[factoryUri] === true &&
    definition.checks.some((check) => check.kind === 'url') &&
    definition.checks.every(
      (check) =>
        check.kind === 'url' ||
        check.kind === 'min' ||
        check.kind === 'max' ||
        check.kind === 'length' ||
        stringLiteralPattern(check) !== undefined
    )
  );
}
