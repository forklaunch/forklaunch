/**
 * shopify-runtime — the small runtime API every Shopify theme's JavaScript
 * assumes is there, answered from the capture instead of 404ing.
 *
 * A Liquid theme is not a static page. Its scripts fetch `/products/<h>.js`
 * for variant data, `/cart.js` to draw the drawer, `?sections=` fragments to
 * re-render after a change, `/recommendations/products.json` for upsells and
 * `/search/suggest.json` for the search box. Serve those with a plain-text
 * 404 and the theme's own code throws mid-initialisation — the product page
 * looks captured (the HTML is all there) but its variant pickers never
 * mount, its accordions never open, and half its images never lazy-load.
 * This is the contract that makes a clone work for a theme nobody tested.
 *
 * Sources: the catalog pull (`catalog/data/<domain>/raw.json`, Shopify's own
 * products.json shape) and the captured pages (for section fragments). A
 * store whose catalog was not pulled simply falls through to the static
 * handler, exactly as before.
 *
 * Analytics sinks (monorail, browsing_context_suggestions) get an empty 2xx:
 * they are Shopify's own telemetry, never data the page renders.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';

const { pageFileFor } = createRequire(import.meta.url)('../urlmap.js') as
  { pageFileFor: (p: string) => { file: string; depth: number } | null };

export interface RuntimeCtx {
  siteRoot: string;
  hasModule: boolean;
  /** Shopify cart.js-shaped cart from the module (bridge mode). */
  cart: () => Promise<any>;
  /** Add via the module (bridge mode). Returns the cart. */
  add?: (handle: string, variantExternalId: string, quantity: number) => Promise<any>;
}

type Raw = { products: any[] };
let rawCache: { root: string; raw: Raw | null; byHandle: Map<string, any>; byVariant: Map<string, any> } | null = null;

function loadRaw(siteRoot: string) {
  if (rawCache && rawCache.root === siteRoot) return rawCache;
  let raw: Raw | null = null;
  try {
    const manifest = JSON.parse(readFileSync(join(dirname(siteRoot), 'manifest.json'), 'utf8'));
    const domain = String(manifest?.source?.domain || '').replace(/^www\./, '');
    const scripts = dirname(import.meta.dir);
    const candidates = [
      join(scripts, 'catalog', 'data', domain.replace(/\./g, '-'), 'raw.json'),
      join(scripts, 'catalog', 'data', ('www.' + domain).replace(/\./g, '-'), 'raw.json'),
    ];
    for (const c of candidates) if (existsSync(c)) { raw = JSON.parse(readFileSync(c, 'utf8')); break; }
  } catch { raw = null; }
  const byHandle = new Map<string, any>();
  const byVariant = new Map<string, any>();
  for (const p of raw?.products || []) {
    byHandle.set(p.handle, p);
    for (const v of p.variants || []) byVariant.set(String(v.id), { product: p, variant: v });
  }
  rawCache = { root: siteRoot, raw, byHandle, byVariant };
  return rawCache;
}

const cents = (s: any) => Math.round(parseFloat(String(s ?? '0')) * 100) || 0;

/** products.json shape -> /products/<handle>.js shape (what theme JS reads). */
function productJs(p: any) {
  const variants = (p.variants || []).map((v: any) => ({
    id: v.id, title: v.title, option1: v.option1 ?? null, option2: v.option2 ?? null, option3: v.option3 ?? null,
    sku: v.sku ?? '', requires_shipping: !!v.requires_shipping, taxable: !!v.taxable,
    featured_image: v.featured_image ?? null, available: v.available !== false,
    name: v.title && v.title !== 'Default Title' ? `${p.title} - ${v.title}` : p.title,
    public_title: v.title && v.title !== 'Default Title' ? v.title : null,
    options: [v.option1, v.option2, v.option3].filter((x: any) => x != null),
    price: cents(v.price), weight: v.grams ?? 0,
    compare_at_price: v.compare_at_price != null ? cents(v.compare_at_price) : null,
    inventory_management: null, barcode: v.barcode ?? null,
    featured_media: v.featured_image ? { id: v.featured_image.id, alt: v.featured_image.alt ?? null, position: v.featured_image.position ?? 1, preview_image: { src: v.featured_image.src } } : null,
    requires_selling_plan: false, selling_plan_allocations: [],
  }));
  const prices = variants.map((v: any) => v.price);
  const images = (p.images || []).map((i: any) => i.src);
  return {
    id: p.id, title: p.title, handle: p.handle, description: p.body_html ?? '', published_at: p.published_at, created_at: p.created_at,
    vendor: p.vendor ?? '', type: p.product_type ?? '', tags: p.tags || [],
    price: Math.min(...prices, Infinity) === Infinity ? 0 : Math.min(...prices),
    price_min: prices.length ? Math.min(...prices) : 0, price_max: prices.length ? Math.max(...prices) : 0,
    available: variants.some((v: any) => v.available), price_varies: new Set(prices).size > 1,
    // Compare-at (the struck-through "Original price") derived from the
    // variants like everything else. Hardcoded null here left every sale card
    // on gorillamind.com without its original price and "20% off" badge.
    ...(() => {
      const cmp = variants.map((v: any) => v.compare_at_price).filter((c: any) => typeof c === 'number' && c > 0);
      return {
        compare_at_price: cmp.length ? Math.min(...cmp) : null,
        compare_at_price_min: cmp.length ? Math.min(...cmp) : 0,
        compare_at_price_max: cmp.length ? Math.max(...cmp) : 0,
        compare_at_price_varies: new Set(cmp).size > 1,
      };
    })(),
    variants, images, featured_image: images[0] ?? null,
    options: (p.options || []).map((o: any) => o.name),
    media: (p.images || []).map((i: any, k: number) => ({ alt: i.alt ?? null, id: i.id, position: i.position ?? k + 1, preview_image: { aspect_ratio: i.width && i.height ? i.width / i.height : 1, height: i.height, width: i.width, src: i.src }, aspect_ratio: i.width && i.height ? i.width / i.height : 1, height: i.height, media_type: 'image', src: i.src, width: i.width })),
    requires_selling_plan: false, selling_plan_groups: [], url: `/products/${p.handle}`,
  };
}

// ---- local cart (no module): enough for a drawer to draw and a count to move
type Line = { id: number; quantity: number; product: any; variant: any };
const localLines: Line[] = [];
function localCart() {
  const items = localLines.map((l) => {
    const price = cents(l.variant.price);
    return {
      id: l.id, properties: {}, quantity: l.quantity, variant_id: l.id, key: `${l.id}:local`,
      title: l.variant.title && l.variant.title !== 'Default Title' ? `${l.product.title} - ${l.variant.title}` : l.product.title,
      price, original_price: price, discounted_price: price, line_price: price * l.quantity, original_line_price: price * l.quantity,
      total_discount: 0, discounts: [], sku: l.variant.sku ?? '', grams: l.variant.grams ?? 0, vendor: l.product.vendor ?? '',
      taxable: !!l.variant.taxable, product_id: l.product.id, product_has_only_default_variant: (l.product.variants || []).length === 1,
      gift_card: false, final_price: price, final_line_price: price * l.quantity, url: `/products/${l.product.handle}?variant=${l.id}`,
      featured_image: { url: (l.variant.featured_image?.src) || (l.product.images?.[0]?.src) || '', aspect_ratio: 1, alt: l.product.title },
      image: (l.variant.featured_image?.src) || (l.product.images?.[0]?.src) || '', handle: l.product.handle, requires_shipping: !!l.variant.requires_shipping,
      product_type: l.product.product_type ?? '', product_title: l.product.title, product_description: '',
      variant_title: l.variant.title && l.variant.title !== 'Default Title' ? l.variant.title : null,
      variant_options: [l.variant.option1, l.variant.option2, l.variant.option3].filter((x) => x != null),
      options_with_values: [], line_level_discount_allocations: [], line_level_total_discount: 0,
    };
  });
  const total = items.reduce((s, i) => s + i.line_price, 0);
  return {
    token: 'local', note: null, attributes: {}, original_total_price: total, total_price: total, total_discount: 0, total_weight: 0,
    item_count: items.reduce((s, i) => s + i.quantity, 0), items, requires_shipping: items.some((i) => i.requires_shipping),
    currency: 'USD', items_subtotal_price: total, cart_level_discount_applications: [],
  };
}

async function readBody(req: Request): Promise<Record<string, any>> {
  const ct = (req.headers.get('content-type') || '').toLowerCase();
  try {
    if (ct.includes('application/json')) return await req.json();
    const text = await req.text();
    const out: Record<string, any> = {};
    for (const [k, v] of new URLSearchParams(text)) {
      // items[0][id]=… / items[0][quantity]=… (Dawn's product form)
      const m = k.match(/^items\[(\d+)\]\[(\w+)\]$/);
      if (m) { (out.items ||= [])[Number(m[1])] ||= {}; out.items[Number(m[1])][m[2]] = v; }
      else out[k] = v;
    }
    return out;
  } catch { return {}; }
}

// ---- section fragments from the captured page --------------------------
function pageHtmlFor(siteRoot: string, p: string): string | null {
  const tries: string[] = [];
  if (p === '/' || p === '') tries.push('index.html');
  else { tries.push(p.replace(/^\//, '').replace(/\/$/, '') + '.html'); const m = pageFileFor(p); if (m) tries.push(m.file); }
  for (const t of tries) { const fp = join(siteRoot, t); if (existsSync(fp)) { try { return readFileSync(fp, 'utf8'); } catch {} } }
  return null;
}
function extractSection(html: string, id: string): string | null {
  // A theme asks for a section by its short name ("drawer-navigation");
  // Shopify resolves that to the section-group instance on the page, whose
  // id carries a prefix ("shopify-section-sections--1898…__drawer-navigation").
  // Match the exact id or that suffix form, on any element the theme used.
  const esc = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const re = new RegExp(`<(div|section|aside|header|footer|nav)\\b[^>]*\\bid="shopify-section-(?:[^"]*__)?${esc}"[^>]*>`, 'i');
  const m = re.exec(html);
  if (!m) return null;
  const tag = m[1].toLowerCase();
  const open = new RegExp(`<${tag}\\b`, 'gi'), close = new RegExp(`</${tag}\\s*>`, 'gi');
  let depth = 0, i = m.index;
  const tok = new RegExp(`<${tag}\\b|</${tag}\\s*>`, 'gi');
  tok.lastIndex = m.index;
  let t: RegExpExecArray | null;
  while ((t = tok.exec(html))) {
    if (t[0][1] === '/') { depth--; if (depth === 0) return html.slice(m.index, t.index + t[0].length); }
    else depth++;
    if (t.index - i > 3_000_000) break;
  }
  void open; void close;
  return null;
}

/** The browse-only cart, for a cart page when no module is wired. */
export function getLocalCart() { return localCart(); }

export async function shopifyRuntime(req: Request, url: URL, p: string, ctx: RuntimeCtx): Promise<Response | null> {
  const method = req.method.toUpperCase();
  const json = (body: any, status = 200) => Response.json(body, { status, headers: { 'cache-control': 'no-store' } });

  // Shopify's own telemetry — never rendered, answer empty and move on.
  if (p.startsWith('/.well-known/shopify/monorail/')) return new Response(null, { status: 204 });
  // Pixel/app scripts read detected_values.country.handle from this for
  // geo detection; an empty object made every one of them fall back noisily.
  if (p === '/browsing_context_suggestions.json') return json({ detected_values: { country: { handle: 'US', name: 'United States' }, currency: { handle: 'USD' }, language: { handle: 'en' } }, suggestions: [] });
  if (p === '/api/unstable/graphql.json' || p.startsWith('/api/') && p.endsWith('/graphql.json')) return json({ data: null, errors: [{ message: 'offline clone' }] }, 200);

  const { raw, byHandle, byVariant } = loadRaw(ctx.siteRoot);

  // ---- section rendering (?sections=a,b -> JSON; ?section_id=a -> HTML) ----
  const sections = url.searchParams.get('sections');
  const sectionId = url.searchParams.get('section_id');
  if (sections || sectionId) {
    const html = pageHtmlFor(ctx.siteRoot, p);
    if (html) {
      if (sectionId) {
        const frag = extractSection(html, sectionId);
        return new Response(frag ?? html, { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
      }
      const out: Record<string, string | null> = {};
      for (const id of String(sections).split(',').map((s) => s.trim()).filter(Boolean)) out[id] = extractSection(html, id);
      return json(out);
    }
  }

  // ---- cart ----
  const cartNow = async () => (ctx.hasModule ? await ctx.cart() : localCart());
  if (method === 'GET' && (p === '/cart.js' || p === '/cart.json')) return json(await cartNow());
  if (method === 'POST' && (p === '/cart/add.js' || p === '/cart/add.json' || p === '/cart/add')) {
    const body = await readBody(req);
    const reqs: { id: string; quantity: number }[] = [];
    if (Array.isArray(body.items)) for (const it of body.items) if (it && it.id) reqs.push({ id: String(it.id), quantity: Number(it.quantity) || 1 });
    else if (body.id) reqs.push({ id: String(body.id), quantity: Number(body.quantity) || 1 });
    if (!reqs.length) return json({ status: 422, message: 'Cart Error', description: 'missing id' }, 422);
    let last: any = null;
    for (const r of reqs) {
      const hit = byVariant.get(r.id);
      if (!hit) return json({ status: 422, message: 'Cart Error', description: `variant ${r.id} not in catalog` }, 422);
      if (ctx.hasModule && ctx.add) last = await ctx.add(hit.product.handle, String(hit.variant.id), r.quantity);
      else {
        const line = localLines.find((l) => l.id === hit.variant.id);
        if (line) line.quantity += r.quantity; else localLines.push({ id: hit.variant.id, quantity: r.quantity, product: hit.product, variant: hit.variant });
      }
    }
    if (ctx.hasModule) return json(last ?? await cartNow());
    const cart = localCart();
    return json(p.endsWith('add.js') && reqs.length === 1 ? cart.items.find((i) => i.variant_id === Number(reqs[0].id)) ?? cart : cart);
  }
  if (method === 'POST' && (p === '/cart/change.js' || p === '/cart/update.js' || p === '/cart/clear.js')) {
    const body = await readBody(req);
    if (!ctx.hasModule) {
      if (p === '/cart/clear.js') localLines.length = 0;
      else if (p === '/cart/change.js') {
        const q = Number(body.quantity);
        const idx = body.line ? Number(body.line) - 1 : localLines.findIndex((l) => String(l.id) === String(body.id) || `${l.id}:local` === String(body.id));
        if (idx >= 0 && localLines[idx]) { if (q > 0) localLines[idx].quantity = q; else localLines.splice(idx, 1); }
      } else if (body.updates && typeof body.updates === 'object') {
        for (const [k, v] of Object.entries(body.updates)) {
          const idx = localLines.findIndex((l) => String(l.id) === String(k) || `${l.id}:local` === String(k));
          const q = Number(v); if (idx >= 0) { if (q > 0) localLines[idx].quantity = q; else localLines.splice(idx, 1); }
        }
      }
    }
    return json(await cartNow());
  }

  if (!raw) return null;

  // ---- products ----
  let m = p.match(/^(?:\/collections\/[^/]+)?\/products\/([^/]+?)\.(js|json)$/);
  if (m && method === 'GET') {
    const prod = byHandle.get(m[1]);
    if (!prod) return json({ error: 'not found' }, 404);
    return json(m[2] === 'js' ? productJs(prod) : { product: prod });
  }
  if (method === 'GET' && (p === '/products.json' || /^\/collections\/[^/]+\/products\.json$/.test(p))) {
    const limit = Math.min(250, Number(url.searchParams.get('limit')) || 30);
    const page = Math.max(1, Number(url.searchParams.get('page')) || 1);
    return json({ products: raw.products.slice((page - 1) * limit, page * limit) });
  }
  // Recommendations asked for as a SECTION (Dawn's product-recommendations
  // element and most themes): the theme fetches its own section rendered
  // with the recommended products and REPLACES the block with the response.
  // Answer with the section as captured on that product's page — the
  // recommendations the live site showed at crawl time — so the block is
  // filled rather than emptied. Falls back to plain cards if the page has no
  // such section.
  if (method === 'GET' && /^\/recommendations\/products(\.json)?$/.test(p) && url.searchParams.get('section_id')) {
    const sid = url.searchParams.get('section_id')!;
    const pid = url.searchParams.get('product_id');
    const prod = pid ? raw.products.find((x) => String(x.id) === String(pid)) : null;
    const html = prod ? pageHtmlFor(ctx.siteRoot, `/products/${prod.handle}`) : null;
    const frag = html ? extractSection(html, sid) : null;
    if (frag && (frag.match(/<a /g) || []).length >= 2) {
      return new Response(frag, { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
    }
    const limit = Math.min(12, Number(url.searchParams.get('limit')) || 4);
    const esc = (t: any) => String(t ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c] as string));
    const picks = raw.products.filter((x) => String(x.id) !== String(pid)).slice(0, limit);
    const cards = picks.map((x) => `<li class="grid__item" style="list-style:none"><a href="/products/${esc(x.handle)}" class="full-unstyled-link" style="display:block;text-decoration:none;color:inherit">${x.images?.[0]?.src ? `<img src="${esc(x.images[0].src)}" alt="${esc(x.title)}" loading="lazy" style="width:100%;height:auto">` : ''}<div style="padding:8px 0"><span>${esc(x.title)}</span><br><span>$${esc(x.variants?.[0]?.price ?? '')}</span></div></a></li>`).join('');
    const body = `<div id="shopify-section-${esc(sid)}" class="shopify-section"><product-recommendations><div class="page-width"><h2 class="h2">Recommended products</h2><ul class="grid product-grid" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:16px;padding:0">${cards}</ul></div></product-recommendations></div>`;
    return new Response(body, { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
  }
  if (method === 'GET' && p === '/recommendations/products.json') {
    const limit = Math.min(20, Number(url.searchParams.get('limit')) || 4);
    const exclude = url.searchParams.get('product_id');
    return json({ products: raw.products.filter((x) => String(x.id) !== exclude).slice(0, limit).map(productJs) });
  }
  // Predictive search asked for as a SECTION (Dawn and most themes:
  // /search/suggest?q=…&section_id=predictive-search expects HTML, and the
  // header drawer's default cards come from exactly this call with an empty
  // q). Serve the captured section; the drawer is then what the crawl saw.
  if (method === 'GET' && /^\/search\/suggest(\.json)?$/.test(p) && url.searchParams.get('section_id')) {
    const sid = url.searchParams.get('section_id')!;
    const html = pageHtmlFor(ctx.siteRoot, p === '/search/suggest' ? '/' : '/') || pageHtmlFor(ctx.siteRoot, '/');
    const frag = html ? extractSection(html, sid) || (html.match(new RegExp(`<[a-z]+\\b[^>]*id="shopify-section-[^"]*${sid.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"[\\s\\S]*?<\\/(?:div|section)>`, 'i')) || [null])[0] : null;
    if (frag) return new Response(frag, { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
  }
  // A real results page for /search?q=… — Shopify renders this server-side,
  // so there is nothing to capture; a clone that 404s the search button is a
  // dead end a client will find in ten seconds. Plain, but every link is real.
  if (method === 'GET' && (p === '/search' || p === '/search/')) {
    const q = (url.searchParams.get('q') || '').trim();
    const ql = q.toLowerCase();
    const hits = ql ? raw.products.filter((x) => `${x.title} ${x.product_type} ${x.vendor} ${(x.tags || []).join(' ')}`.toLowerCase().includes(ql)) : [];
    const esc = (t: any) => String(t ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c] as string));
    const cards = hits.slice(0, 48).map((x) => `<li style="list-style:none;margin:0 0 18px"><a href="/products/${esc(x.handle)}" style="display:flex;gap:14px;align-items:center;text-decoration:none;color:inherit">${x.images?.[0]?.src ? `<img src="${esc(x.images[0].src)}" alt="" width="72" height="72" style="object-fit:cover;border-radius:6px">` : ''}<span><strong>${esc(x.title)}</strong><br><span>$${esc(x.variants?.[0]?.price ?? '')}</span></span></a></li>`).join('');
    const body = `<!doctype html><html><head><meta charset="utf-8"><title>Search${q ? ': ' + esc(q) : ''}</title><meta name="viewport" content="width=device-width,initial-scale=1"></head><body style="font:16px/1.4 system-ui,sans-serif;max-width:720px;margin:40px auto;padding:0 20px"><form action="/search" method="get" role="search" style="margin-bottom:24px"><input type="search" name="q" value="${esc(q)}" placeholder="Search" style="font:inherit;padding:10px 12px;width:70%"> <button type="submit" style="font:inherit;padding:10px 16px">Search</button></form><h1 style="font-size:22px">${q ? `${hits.length} result${hits.length === 1 ? '' : 's'} for “${esc(q)}”` : 'Search'}</h1><ul style="padding:0">${cards}</ul><p><a href="/">← Back to the store</a></p></body></html>`;
    return new Response(body, { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
  }
  if (method === 'GET' && (p === '/search/suggest.json' || p === '/search/suggest')) {
    const q = (url.searchParams.get('q') || '').toLowerCase().trim();
    const limit = Math.min(10, Number(url.searchParams.get('resources[limit]')) || 10);
    const hits = q ? raw.products.filter((x) => `${x.title} ${x.product_type} ${(x.tags || []).join(' ')}`.toLowerCase().includes(q)).slice(0, limit) : [];
    return json({ resources: { results: { products: hits.map((x) => ({ available: true, body: x.body_html ?? '', compare_at_price_max: '0', compare_at_price_min: '0', handle: x.handle, id: x.id, image: x.images?.[0]?.src ?? '', price: x.variants?.[0]?.price ?? '0', price_max: x.variants?.[0]?.price ?? '0', price_min: x.variants?.[0]?.price ?? '0', tags: x.tags || [], title: x.title, type: x.product_type ?? '', url: `/products/${x.handle}`, vendor: x.vendor ?? '', variants: [] })), collections: [], pages: [], articles: [], queries: [] } } });
  }
  return null;
}
