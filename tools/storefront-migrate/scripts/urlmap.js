/**
 * urlmap — the single source of truth for how a storefront URL maps to a local
 * output file. Imported by BOTH crawl.js (to decide where each captured page is
 * written) and check-complete.mjs (to verify every public page got captured).
 * Sharing one copy is deliberate: if these two ever disagreed, the fidelity
 * check could pass while the real capture put pages elsewhere — a false green.
 * check-complete.test.mjs pins the behavior so it can't drift.
 */
const SKIP_PATH = /^\/(cart|checkout|account|orders|search|apps|admin|cdn|_a|assets|api|services|tools|challenge|password|a\/|wpm@|\.well-known)(\/|$)/i;
const safe = (s) => s.replace(/[^A-Za-z0-9._-]/g, '_').slice(0, 80);

// A storefront path -> { file, depth } (depth drives relative-link rewriting),
// or null for paths we never capture (cart/account/assets/etc.).
function pageFileFor(pathname) {
  const p = pathname.replace(/\/+$/, '') || '/';
  if (p === '/') return { file: 'index.html', depth: 0 };
  let m = p.match(/^\/products\/([^/]+)/);
  if (m) return { file: `products/${safe(m[1])}.html`, depth: 1 };
  m = p.match(/^\/collections\/([^/]+)\/products\/([^/]+)/);
  if (m) return { file: `products/${safe(m[2])}.html`, depth: 1 };
  m = p.match(/^\/collections\/([^/]+)/);
  if (m) return { file: `collections/${safe(m[1])}.html`, depth: 1 };
  m = p.match(/^\/pages\/([^/]+)/);
  if (m) return { file: `pages/${safe(m[1])}.html`, depth: 1 };

  // Generic fallback for stores that rewrite their URLs (e.g. /bath, /bedding).
  if (SKIP_PATH.test(p)) return null;
  const segs = p.split('/').filter(Boolean);
  if (!segs.length || segs.length > 3) return null;
  if (/\.[a-z0-9]{2,5}$/i.test(segs[segs.length - 1])) return null;  // it's a file
  if (segs.length === 1) return { file: `${safe(segs[0])}.html`, depth: 0 };
  return { file: `${safe(segs[0])}/${safe(segs.slice(1).join('-'))}.html`, depth: 1 };
}

// Page classification for the manifest (mirrors pageFileFor's precedence).
function pageTypeFor(pathname) {
  if (pathname === '/') return 'home';
  if (/\/products\//.test(pathname)) return 'product';
  if (/^\/collections\//.test(pathname)) return 'collection';
  return 'page';
}

module.exports = { pageFileFor, pageTypeFor, safe, SKIP_PATH };
