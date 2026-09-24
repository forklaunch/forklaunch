import { LicenseScope } from '@forklaunch/interfaces-mlse/types';

// Licenses whose terms allow storing, adapting and showing the full text in a
// commercial product. Anything not listed, including every NonCommercial and
// NoDerivatives variant, falls back to metadata only.
const FULL_TEXT_LICENSES = new Set([
  'cc0',
  'cc-by',
  'cc-by-sa',
  'public-domain',
  'us-government-work'
]);

// Copyrighted by default, but may be indexed and quoted briefly with a link
// to the original (PubMed abstracts).
const EXCERPT_ONLY_LICENSES = new Set(['publisher-copyright-abstract']);

// Maps the many spellings sources use ("CC BY 4.0", "cc-by",
// "http://creativecommons.org/licenses/by-sa/3.0/", "Public Domain") onto one
// comparable token.
export function normalizeLicense(license: string | undefined | null): string {
  if (!license || !license.trim()) {
    return 'unknown';
  }

  let token = license
    .trim()
    .toLowerCase()
    .replace(/^https?:\/\/(www\.)?creativecommons\.org\/(licenses|publicdomain)\//, '')
    .replace(/[\s_/]+/g, '-')
    .replace(/^creative-commons-/, 'cc-')
    .replace(/-+$/, '');

  // drop a trailing version and optional jurisdiction/IGO port: -4.0,
  // -3.0-igo, -2.5-us. The hyphen is required so the "0" in "cc0" survives.
  token = token.replace(/-\d+(\.\d+)*(-[a-z]{2,4})?$/, '').replace(/-+$/, '');

  // creativecommons.org URLs name the license without the "cc-" prefix
  if (/^by(-|$)/.test(token)) {
    token = `cc-${token}`;
  }
  if (token === 'zero' || token === 'cc-zero') {
    return 'cc0';
  }
  if (token === 'mark' || token === 'public-domain-mark') {
    return 'public-domain';
  }
  return token;
}

// Decides what MLSE may keep from a document, before anything is written.
// Unknown or missing licenses are treated as the most restrictive case.
export function licenseScopeFor(
  license: string | undefined | null
): LicenseScope {
  const normalized = normalizeLicense(license);
  if (FULL_TEXT_LICENSES.has(normalized)) {
    return 'full_text';
  }
  if (EXCERPT_ONLY_LICENSES.has(normalized)) {
    return 'excerpt_only';
  }
  return 'metadata_only';
}
