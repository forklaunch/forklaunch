/** Token-bearing redirects require an exact, configured browser origin. Wildcards never qualify. */
export function trustedAuthCallback(
  value: unknown,
  origins: readonly string[]
): URL | null {
  if (typeof value !== 'string' || !value || /[\\\s]/.test(value)) return null;
  try {
    const url = new URL(value);
    if (
      !['https:', 'http:'].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.hash
    )
      return null;
    const trusted = origins.some((origin) => {
      try {
        const allowed = new URL(origin.trim());
        return (
          !allowed.username &&
          !allowed.password &&
          !allowed.search &&
          !allowed.hash &&
          allowed.pathname === '/' &&
          !allowed.hostname.includes('*') &&
          allowed.origin === url.origin
        );
      } catch {
        return false;
      }
    });
    return trusted ? url : null;
  } catch {
    return null;
  }
}

/** Never construct credential-bearing internal fetches from a request's Host header. */
export function configuredAuthOrigin(value: unknown): string {
  if (typeof value !== 'string')
    throw new Error('Authentication base URL is not configured.');
  const url = new URL(value);
  if (
    !['https:', 'http:'].includes(url.protocol) ||
    url.username ||
    url.password
  )
    throw new Error('Authentication base URL is invalid.');
  return url.origin;
}

/** JSON string escaping alone does not stop a closing script tag in HTML. */
export function inlineScriptJson(value: unknown): string {
  return JSON.stringify(value)
    .replace(/</g, '\\u003c')
    .replace(/>/g, '\\u003e')
    .replace(/&/g, '\\u0026')
    .replace(/\u2028/g, '\\u2028')
    .replace(/\u2029/g, '\\u2029');
}
