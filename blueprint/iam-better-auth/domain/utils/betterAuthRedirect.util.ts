import { URL } from 'node:url';

export interface TrustedRedirectContext {
  baseURL: string;
  isTrustedOrigin(url: string): boolean;
}

/** Apply the same configured origin policy as Better Auth's OAuth endpoints. */
export function trustedCallbackUrl(
  value: string,
  context: TrustedRedirectContext
): URL | null {
  try {
    const callback = new URL(value, new URL('/', context.baseURL));
    if (
      !['http:', 'https:'].includes(callback.protocol) ||
      callback.username ||
      callback.password ||
      !context.isTrustedOrigin(callback.href)
    ) {
      return null;
    }
    return callback;
  } catch {
    return null;
  }
}

/** JSON literals embedded in HTML must not contain an HTML script terminator. */
export function scriptJson(value: unknown): string {
  const json = JSON.stringify(value);
  if (json === undefined) {
    throw new TypeError('A script literal must be JSON serializable');
  }
  return json.replace(/[<>&\u2028\u2029]/g, (character) => {
    const code = character.charCodeAt(0).toString(16).padStart(4, '0');
    return `\\u${code}`;
  });
}
