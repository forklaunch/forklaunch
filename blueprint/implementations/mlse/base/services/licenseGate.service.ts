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

// Contract terms set by LicensedContentAdapter. Honoured only when the caller
// says the document came through the adapter: a public feed carrying the
// same string gets nothing.
const CONTRACT_LICENSES: Record<string, LicenseScope> = {
  'licensed-full-text': 'full_text',
  'licensed-excerpt': 'excerpt_only'
};

// Real license strings are short. Anything longer is not a license we
// recognize, and capping it keeps normalization cheap on hostile input.
const MAX_LICENSE_LENGTH = 200;
const VERSION_PART = /^\d+(\.\d+)*$/;
const JURISDICTION_PART = /^[a-z]{2,4}$/;

// Maps the many spellings sources use ("CC BY 4.0", "cc-by",
// "http://creativecommons.org/licenses/by-sa/3.0/", "Public Domain") onto one
// comparable token.
//
// License strings come from external documents, so this avoids regular
// expressions with unanchored repetition (e.g. /-+$/), which backtrack
// polynomially on long runs of '-'. The string is split on hyphens and the
// trailing version handled part by part instead.
export function normalizeLicense(license: string | undefined | null): string {
  const trimmed = license?.trim();
  if (!trimmed || trimmed.length > MAX_LICENSE_LENGTH) {
    return 'unknown';
  }

  const parts = trimmed
    .toLowerCase()
    .replace(/^https?:\/\/(www\.)?creativecommons\.org\/(licenses|publicdomain)\//, '')
    .replace(/[\s_/]+/g, '-')
    .split('-')
    .filter((part) => part.length > 0);

  if (parts[0] === 'creative' && parts[1] === 'commons') {
    parts.splice(0, 2, 'cc');
  }

  // drop a trailing version and optional jurisdiction/IGO port: 4.0,
  // 3.0-igo, 2.5-us. Only whole parts are removed, so "cc0" keeps its "0".
  const last = parts.length - 1;
  if (
    parts.length > 2 &&
    JURISDICTION_PART.test(parts[last]) &&
    VERSION_PART.test(parts[last - 1])
  ) {
    parts.pop();
  }
  while (parts.length > 1 && VERSION_PART.test(parts[parts.length - 1])) {
    parts.pop();
  }

  let token = parts.join('-');
  if (!token) {
    return 'unknown';
  }

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
  license: string | undefined | null,
  options: { viaLicensedAdapter?: boolean } = {}
): LicenseScope {
  const normalized = normalizeLicense(license);
  const contract = CONTRACT_LICENSES[normalized];
  if (contract) {
    return options.viaLicensedAdapter ? contract : 'metadata_only';
  }
  if (FULL_TEXT_LICENSES.has(normalized)) {
    return 'full_text';
  }
  if (EXCERPT_ONLY_LICENSES.has(normalized)) {
    return 'excerpt_only';
  }
  return 'metadata_only';
}
