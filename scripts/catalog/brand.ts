/**
 * Best-effort brand extraction from a storefront's public homepage — a mark
 * (favicon), primary color, site name. Used only to make the local demo
 * storefront (src/server.ts + views.ts) look recognizably like the source
 * store for a sales demo. This is NOT theme cloning: no CSS/layout/JS is
 * scraped or reproduced, only a handful of brand assets already served
 * publicly by the store itself (same assets a browser loads to render the
 * page). Every field is optional and falls back to a neutral default if not
 * found — a missing brand asset should never break the migration.
 *
 * Deliberately does NOT try to guess "the logo" out of arbitrary <img> tags
 * on the page — tried it, and across two real stores it confidently picked
 * a background texture and a t-shirt product photo respectively (both had
 * "logo" somewhere in class/alt/filename, neither was the actual mark).
 * Generic Shopify themes vary too much for that heuristic to be trustworthy,
 * and a wrong "logo" in a client demo is worse than no logo. The favicon is
 * the one brand mark that's reliably the real thing on every site, since
 * it's exactly what already shows in a browser tab.
 */
export interface Brand {
  name: string | null;
  markSrc: string | null;
  primaryColor: string | null;
  /** A real banner/hero image from the store's own homepage (its og:image —
   *  the same image the store already serves for social-media link
   *  previews, so it's reliably a real, representative shot, not a guess). */
  heroImageSrc: string | null;
  /** The store's actual webfont, when it's loaded via a Google Fonts
   *  stylesheet link — a structured, reliably-parseable URL format, unlike
   *  guessing a font out of freeform theme CSS (which we don't attempt,
   *  same reasoning as skipping logo-by-class-name: too theme-specific to
   *  trust). Null just means the demo uses a clean system-font fallback. */
  fontFamily: string | null;
  fontStylesheetHref: string | null;
}

function resolveUrl(maybeRelative: string, base: string): string {
  try {
    return new URL(maybeRelative, base).toString();
  } catch {
    return maybeRelative;
  }
}

function matchAttr(html: string, tagPattern: RegExp, attr: string): string | null {
  const tag = html.match(tagPattern);
  if (!tag) return null;
  const attrMatch = tag[0].match(new RegExp(`${attr}=["']([^"']+)["']`, 'i'));
  return attrMatch ? attrMatch[1] : null;
}

export async function extractBrand(shopUrl: string): Promise<Brand> {
  const base = shopUrl.replace(/\/$/, '');
  let html = '';
  try {
    const res = await fetch(base, { headers: { 'user-agent': 'forklaunch-migrate/0.1' } });
    if (res.ok) html = await res.text();
  } catch {
    // Network failure or non-HTML response — return an empty brand rather
    // than fail the migration over a cosmetic step.
    return { name: null, markSrc: null, primaryColor: null, heroImageSrc: null, fontFamily: null, fontStylesheetHref: null };
  }

  const ogSiteName = matchAttr(html, /<meta[^>]+property=["']og:site_name["'][^>]*>/i, 'content');
  const titleTag = html.match(/<title[^>]*>([^<]*)<\/title>/i)?.[1]?.trim() ?? null;
  const name = ogSiteName || titleTag || null;

  const favicon =
    matchAttr(html, /<link[^>]+rel=["']apple-touch-icon["'][^>]*>/i, 'href') ??
    matchAttr(html, /<link[^>]+rel=["'](?:shortcut )?icon["'][^>]*>/i, 'href');
  const markSrc = favicon ? resolveUrl(favicon, base) : null;

  const themeColor = matchAttr(html, /<meta[^>]+name=["']theme-color["'][^>]*>/i, 'content');
  const primaryColor = themeColor && /^#[0-9a-f]{3,8}$/i.test(themeColor) ? themeColor : null;

  const ogImage = matchAttr(html, /<meta[^>]+property=["']og:image["'][^>]*>/i, 'content');
  const heroImageSrc = ogImage ? resolveUrl(ogImage, base) : null;

  // Google Fonts stylesheet links have a fixed, parseable shape:
  // https://fonts.googleapis.com/css2?family=Inter:wght@400;700&display=swap
  const fontLinkMatch = html.match(/<link[^>]+href=["'](https:\/\/fonts\.googleapis\.com\/css2?\?[^"']+)["'][^>]*>/i);
  const fontStylesheetHref = fontLinkMatch ? fontLinkMatch[1].replace(/&amp;/g, '&') : null;
  const familyParam = fontStylesheetHref ? new URL(fontStylesheetHref).searchParams.get('family') : null;
  // family param looks like "Inter:wght@400;700" or "Playfair+Display:ital@0;1" — just the name before ':'.
  const fontFamily = familyParam ? familyParam.split(':')[0].replace(/\+/g, ' ') : null;

  return { name, markSrc, primaryColor, heroImageSrc, fontFamily, fontStylesheetHref };
}
