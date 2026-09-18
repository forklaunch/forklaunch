/**
 * Server-rendered demo storefront. Per the ownership boundary, the real
 * storefront is Guild's build — this exists only to SHOW the migrated
 * catalog and drive the purchase loop in a browser, for a sales demo. It
 * optionally picks up a Brand (src/brand.ts: logo, primary color, site name
 * scraped from the source store's own public homepage) so the demo reads as
 * "this store," not a generic placeholder — without reproducing the source
 * site's actual theme/layout/CSS.
 */
import type { Brand } from './brand.ts';

const money = (c: number) => `$${(c / 100).toFixed(2)}`;
// Titles, descriptions, vendors and image URLs are the merchant's own text;
// every one is escaped before it is placed in markup or an attribute.
const esc = (t: unknown) => String(t ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c] as string));

const DEFAULT_PRIMARY = '#0f5a8a';
const DEFAULT_HEADER = '#11324d';

/** Derives a darker header shade from the brand's primary color so the
 *  header and buttons read as one palette instead of two unrelated colors.
 *  Falls back to the default navy when there's no brand color to work from. */
function headerShade(primary: string | null): string {
  if (!primary) return DEFAULT_HEADER;
  const hex = primary.replace('#', '');
  if (hex.length !== 6) return DEFAULT_HEADER;
  const [r, g, b] = [0, 2, 4].map((i) => Math.round(parseInt(hex.slice(i, i + 2), 16) * 0.55));
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, '0')).join('')}`;
}

function layout(title: string, body: string, shop: string, brand?: Brand | null): string {
  const primary = brand?.primaryColor || DEFAULT_PRIMARY;
  const header = headerShade(brand?.primaryColor ?? null);
  const brandLabel = brand?.markSrc
    ? `<img src="${esc(brand.markSrc)}" alt="" style="height:28px;width:28px;border-radius:6px;object-fit:contain"><b>${esc(brand?.name ?? shop)}</b>`
    : `<b>${esc(brand?.name ?? shop)}</b>`;
  const favicon = brand?.markSrc ? `<link rel="icon" href="${brand.markSrc}">` : '';
  const fontLink = brand?.fontStylesheetHref ? `<link rel="stylesheet" href="${brand.fontStylesheetHref}">` : '';
  const bodyFont = brand?.fontFamily
    ? `"${brand.fontFamily}", -apple-system, Arial, sans-serif`
    : '-apple-system, Arial, sans-serif';
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(title)}</title>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  ${favicon}
  ${fontLink}
  <style>
    body{font-family:${bodyFont};margin:0;background:#fafafa;color:#1c2530}
    header{background:${header};color:#fff;padding:14px 24px;display:flex;justify-content:space-between;align-items:center}
    header a{color:#fff;text-decoration:none;display:flex;align-items:center;gap:10px}
    .wrap{max-width:1000px;margin:0 auto;padding:24px}
    .hero{position:relative;height:280px;background:#e3e9ef;background-size:cover;background-position:center;display:flex;align-items:flex-end;margin-bottom:24px;border-radius:10px;overflow:hidden}
    .hero::after{content:'';position:absolute;inset:0;background:linear-gradient(0deg,rgba(0,0,0,.55),rgba(0,0,0,0) 60%)}
    .hero h1{position:relative;color:#fff;padding:20px;margin:0;font-size:28px}
    .nav{display:flex;gap:8px;flex-wrap:wrap;margin-bottom:18px}
    .nav a{padding:6px 14px;border-radius:16px;background:#fff;border:1px solid #e3e9ef;color:#1c2530;text-decoration:none;font-size:13px}
    .nav a.active{background:${primary};color:#fff;border-color:${primary}}
    .grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:18px}
    .card{background:#fff;border:1px solid #e3e9ef;border-radius:8px;overflow:hidden}
    .card img{width:100%;height:180px;object-fit:cover;background:#eee}
    .card .p{padding:10px}
    .card .t{font-size:13px;font-weight:600;line-height:1.3}
    .card .v{font-size:12px;color:#5a6b7b}
    a.btn,button{background:${primary};color:#fff;border:0;border-radius:6px;padding:8px 14px;font-size:14px;cursor:pointer;text-decoration:none;display:inline-block}
    .muted{color:#5a6b7b;font-size:13px}
    table{width:100%;border-collapse:collapse}td,th{text-align:left;padding:8px;border-bottom:1px solid #eee}
    .sale{color:#b12704;font-weight:600}.was{text-decoration:line-through;color:#999;font-size:12px}
    .badge{display:inline-block;background:#eef4f8;color:${primary};border-radius:10px;padding:1px 8px;font-size:11px}
  </style></head><body>
  <header><a href="/">${brandLabel}<span class="muted" style="color:#cfe0ee">migrated demo</span></a><a href="/cart">🛒 Cart</a></header>
  <div class="wrap">${body}</div></body></html>`;
}

export function productGrid(shop: string, products: any[], brand?: Brand | null, activeType?: string | null, allTypes?: string[]): string {
  const types = allTypes ?? [...new Set(products.map((p) => p.product_type).filter(Boolean))].sort();
  const nav = types.length > 1
    ? `<div class="nav"><a href="/" class="${!activeType ? 'active' : ''}">All</a>${types.map((t) =>
        `<a href="/?type=${encodeURIComponent(t)}" class="${activeType === t ? 'active' : ''}">${t}</a>`).join('')}</div>`
    : '';
  const hero = brand?.heroImageSrc
    ? `<div class="hero" style="background-image:url('${esc(brand.heroImageSrc)}')"><h1>${esc(brand?.name ?? shop)}</h1></div>`
    : '';
  const cards = products.map((p) => `
    <a class="card" href="/products/${esc(p.handle)}">
      <img src="${esc(p.image_src || '')}" alt="">
      <div class="p"><div class="t">${esc(p.title)}</div><div class="v">${esc(p.product_type || p.vendor || '')}</div></div>
    </a>`).join('');
  return layout(brand?.name ?? shop, `${hero}${nav}<h2>${products.length} products</h2><div class="grid">${cards}</div>`, shop, brand);
}

export function productDetail(shop: string, p: any, brand?: Brand | null): string {
  const opts = p.options.map((o: any) => o.name + (o.isPackQuantity ? ' <span class="badge">pack qty</span>' : '')).join(', ');
  const rows = p.variants.map((v: any) => {
    const price = v.compare_at_price_cents && v.compare_at_price_cents > v.price_cents
      ? `<span class="sale">${money(v.price_cents)}</span> <span class="was">${money(v.compare_at_price_cents)}</span>`
      : money(v.price_cents);
    const oos = v.stock <= 0;
    return `<tr><td>${esc(v.title)}</td><td>${price}</td><td>${oos ? 'out of stock' : v.stock + ' in stock'}</td>
      <td><form method="POST" action="/cart/add"><input type="hidden" name="variant_id" value="${v.id}">
      <button ${oos ? 'disabled' : ''}>Add to cart</button></form></td></tr>`;
  }).join('');
  return layout(p.title, `
    <p class="muted"><a href="/">← all products</a></p>
    <h1>${esc(p.title)}</h1>
    <p class="muted">${esc(p.vendor)} · ${esc(p.product_type)} · options: ${esc(opts || 'none')}</p>
    <table><tr><th>Variant</th><th>Price</th><th>Stock</th><th></th></tr>${rows}</table>
    <details><summary class="muted">description</summary>${esc(p.description_html || '')}</details>`, shop, brand);
}

export function cartView(shop: string, cart: any, brand?: Brand | null): string {
  if (cart.items.length === 0) return layout('Cart', `<h1>Cart</h1><p class="muted">Empty. <a href="/">Shop →</a></p>`, shop, brand);
  const rows = cart.items.map((it: any) => `<tr><td>${esc(it.product_title)}<br><span class="muted">${esc(it.variant_title)}</span></td>
    <td>${it.quantity}</td><td>${money(it.price_cents * it.quantity)}</td></tr>`).join('');
  return layout('Cart', `
    <h1>Cart</h1>
    <table><tr><th>Item</th><th>Qty</th><th>Total</th></tr>${rows}</table>
    <p><b>Subtotal: ${money(cart.subtotalCents)}</b> <span class="muted">(tax added at checkout)</span></p>
    <form method="POST" action="/checkout"><button>Checkout →</button></form>`, shop, brand);
}

export function orderView(shop: string, order: any, brand?: Brand | null): string {
  const rows = order.items.map((it: any) => `<tr><td>${esc(it.product_title)} — ${esc(it.variant_title)}</td>
    <td>${it.quantity}</td><td>${money(it.unit_price_cents * it.quantity)}</td></tr>`).join('');
  return layout('Order', `
    <h1>Order #${order.id} — <span class="badge">${order.status}</span></h1>
    <table><tr><th>Item</th><th>Qty</th><th>Total</th></tr>${rows}</table>
    <p class="muted">Subtotal ${money(order.subtotal_cents)} · Tax ${money(order.tax_cents)}</p>
    <p><b>Total: ${money(order.total_cents)}</b></p>
    ${order.status === 'pending' ? `<form method="POST" action="/orders/${order.id}/pay"><button>Pay now (mock)</button></form>` : ''}
    <p class="muted"><a href="/">← keep shopping</a></p>`, shop, brand);
}
