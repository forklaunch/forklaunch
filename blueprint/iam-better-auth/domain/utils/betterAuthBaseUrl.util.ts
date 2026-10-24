import { URL } from 'node:url';

/** Match Better Auth's configured URL resolution for provider callbacks. */
export function resolveBetterAuthBaseUrl(
  baseURL: string,
  basePath: string
): string {
  if (new URL(baseURL).pathname.replace(/\/+$/, '')) {
    return baseURL;
  }
  const origin = baseURL.replace(/\/+$/, '');
  if (!basePath || basePath === '/') {
    return origin;
  }
  return `${origin}${basePath.startsWith('/') ? basePath : `/${basePath}`}`;
}
