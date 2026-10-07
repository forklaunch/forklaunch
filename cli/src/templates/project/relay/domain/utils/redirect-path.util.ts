/**
 * File: redirect-path.util.ts
 *
 * Open-redirect guard for the managed-apps OAuth relay handoff. The browser is
 * 302'd to whatever path a relay session declared, so the destination must be
 * proven root-relative before it is ever used in a `Location` header. Anything
 * that could point at another origin collapses to the instance root.
 */

/** The instance root; the safe destination when nothing better is trusted. */
export const DEFAULT_REDIRECT_PATH = '/';

/**
 * Accept only root-relative paths. Rejects absolute URLs, protocol-relative
 * `//host`, and the `/\host` backslash trick some browsers treat as
 * protocol-relative. Everything untrusted becomes `/`.
 */
export function sanitizeRedirectPath(
  candidate: string | null | undefined
): string {
  if (typeof candidate !== 'string' || candidate.length === 0) {
    return DEFAULT_REDIRECT_PATH;
  }
  if (!candidate.startsWith('/')) {
    return DEFAULT_REDIRECT_PATH;
  }
  // `//host` and `/\host` are protocol-relative to another origin.
  if (candidate.startsWith('//') || candidate.startsWith('/\\')) {
    return DEFAULT_REDIRECT_PATH;
  }
  return candidate;
}
