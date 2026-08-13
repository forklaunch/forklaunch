#!/usr/bin/env bun
/**
 * heroserve-fl — serve a captured (--backend) storefront with its cart and
 * checkout wired to the REAL ForkLaunch ecommerce-stripe module over HTTP.
 *
 * This is the production wiring (as opposed to heroserve-pg.ts, which talks to
 * the local Postgres scaffold): a shopper browses the migrated visual clone,
 * and every add-to-cart / checkout call is proxied to the running ForkLaunch
 * module — the same module the catalog was imported into — authenticated with
 * the module's own HMAC scheme (@forklaunch/core's createHmacToken, reproduced
 * here so the skill stays self-contained).
 *
 *   bun heroserve-fl.ts <site-dir> <port> <module-url> <hmac-secret>
 *
 * The captured product page keeps its native Shopify add-to-cart form, whose
 * hidden `id` input is the source-platform (Shopify) variant id. The module
 * stored that as each variant's externalId, so we map handle + shopify-variant
 * -> the module's own variant UUID (via GET /product/handle/:handle) before
 * adding to the module cart. Payment (Stripe) is initiated by /checkout and
 * needs a real STRIPE_API_KEY in the module's env to complete the charge; the
 * order itself is created regardless.
 */
import { existsSync, readFileSync, statSync } from 'node:fs';
import { join, extname } from 'node:path';
import { createHmac, randomUUID } from 'node:crypto';

const [siteRoot, portArg, moduleUrl, secret] = process.argv.slice(2);
const PORT = Number(portArg ?? 4700);
const MODULE = (moduleUrl ?? 'http://localhost:8001').replace(/\/$/, '');
const SECRET = secret ?? process.env.HMAC_SECRET_KEY ?? '';

// --- HMAC exactly as @forklaunch/core's createHmacToken builds it -----------
// message = `${method}\n${path}\n${bodyString}${timestamp}\n${nonce}` where
// bodyString = body ? JSON.stringify(body)+'\n' : the literal string 'undefined'
// (a no-body request signs `${undefined}`, which stringifies to "undefined").
// `path` is the handler path RELATIVE to its router mount, not the request URL.
function authHeader(method: string, signedPath: string, body?: unknown): string {
  const ts = new Date();
  const nonce = randomUUID();
  const bodyString = body != null ? `${JSON.stringify(body)}\n` : 'undefined';
  const sig = createHmac('sha256', SECRET)
    .update(`${method}\n${signedPath}\n${bodyString}${ts.toISOString()}\n${nonce}`)
    .digest('base64');
  return `HMAC keyId=default ts=${ts.toISOString()} nonce=${nonce} signature=${sig}`;
}

async function mod(method: string, url: string, signedPath: string, body?: unknown) {
  const res = await fetch(MODULE + url, {
    method,
    headers: { 'content-type': 'application/json', authorization: authHeader(method, signedPath, body) },
    body: body != null ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  let json: any; try { json = JSON.parse(text); } catch { json = text; }
  return { code: res.status, body: json };
}

// One demo cart per server run (mirrors heroserve-pg's single-cart model).
let cartId: string | null = null;
async function ensureCart(): Promise<string> {
  if (cartId) return cartId;
  const r = await mod('POST', '/cart', '/', { customerId: 'fl-demo-customer' });
  cartId = r.body?.id;
  return cartId!;
}

// Map a source-platform variant id (from the captured page) to the module's
// own variant UUID via the product's handle.
async function resolveVariant(handle: string, shopifyVariantId?: string): Promise<string | null> {
  const p = await mod('GET', `/product/handle/${handle}`, `/handle/${handle}`);
  if (p.code !== 200) return null;
  const productId = p.body.id;
  const vs = await mod('GET', `/variant/product/${productId}`, `/product/${productId}`);
  if (vs.code !== 200 || !Array.isArray(vs.body) || !vs.body.length) return null;
  if (shopifyVariantId) {
    const hit = vs.body.find((v: any) => String(v.externalId) === String(shopifyVariantId));
    if (hit) return hit.id;
  }
  return vs.body[0].id; // fall back to the first variant
}

const MIME: Record<string, string> = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css',
  '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.avif': 'image/avif', '.gif': 'image/gif',
  '.woff': 'font/woff', '.woff2': 'font/woff2', '.ico': 'image/x-icon',
};

// Client shim injected into every served HTML page: intercept the native
// Shopify cart calls and the checkout click, and route them to /__fl/*.
const SHIM = `<script>(function(){
  function handle(){var m=location.pathname.match(/\\/products\\/([^/?#]+?)(?:\\.html)?$/);return m?m[1]:'';}
  var of=window.fetch;
  window.fetch=function(u,o){
    try{
      var url=(''+((u&&u.url)||u));
      if(/\\/cart\\/add(\\.js)?/.test(url)){
        var id='';try{var b=o&&o.body;if(b instanceof FormData){id=b.get('id')}else if(typeof b==='string'){var mm=b.match(/[?&]?id=([^&]+)/);id=mm?decodeURIComponent(mm[1]):(JSON.parse(b).id||'')}}catch(e){}
        return of('/__fl/add',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({handle:handle(),variantExternalId:String(id||'')})}).then(function(r){return r.json()}).then(function(c){flToast();return new Response(JSON.stringify(c),{headers:{'content-type':'application/json'}})});
      }
      if(/\\/cart(\\.js|\\.json)(\\?|$)/.test(url))return of('/__fl/cart');
    }catch(e){}
    return of(u,o);
  };
  function flToast(){var t=document.getElementById('fl-toast');if(!t){t=document.createElement('div');t.id='fl-toast';t.style.cssText='position:fixed;top:16px;right:16px;z-index:2147483000;background:#111;color:#fff;padding:12px 18px;border-radius:10px;font:600 14px system-ui;box-shadow:0 6px 20px rgba(0,0,0,.3)';document.body.appendChild(t)}t.innerHTML='\\u2713 Added to cart &nbsp; <a href="/__fl/checkout" style="color:#8bf">Checkout \\u2192</a>';}
  document.addEventListener('submit',function(e){var f=e.target;if(f&&f.action&&/\\/cart\\/add/.test(f.action)){e.preventDefault();var fd=new FormData(f);window.fetch('/cart/add.js',{method:'POST',body:fd});}},true);
  document.addEventListener('click',function(e){var a=e.target.closest&&e.target.closest('[name="checkout"],[href*="/checkout"],[href="/cart"]');if(a){e.preventDefault();location.href='/__fl/checkout';}},true);
})();</script>`;

function serveFile(fp: string): Response | null {
  if (!existsSync(fp) || statSync(fp).isDirectory()) return null;
  const ext = extname(fp).toLowerCase();
  let body: Buffer | string = readFileSync(fp);
  if (ext === '.html') {
    body = body.toString('utf8').replace(/<\/body>/i, SHIM + '</body>');
  }
  return new Response(body, { headers: { 'content-type': MIME[ext] || 'application/octet-stream' } });
}

function money(c: number) { return '$' + ((c || 0) / 100).toFixed(2); }

Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);
    const p = decodeURIComponent(url.pathname);
    try {
      // ---- storefront -> module bridge ----
      if (p === '/__fl/add' && req.method === 'POST') {
        const { handle, variantExternalId } = await req.json();
        const variantId = await resolveVariant(handle, variantExternalId);
        if (!variantId) return Response.json({ item_count: 0, error: 'variant not found' }, { status: 404 });
        const cid = await ensureCart();
        await mod('POST', '/cart/items', '/items', { cartId: cid, variantId, quantity: 1 });
        return Response.json(await shopifyCart());
      }
      if (p === '/__fl/cart') return Response.json(await shopifyCart());
      if (p === '/__fl/checkout') {
        const cid = await ensureCart();
        const addr = { name: 'ForkLaunch Demo', line1: '500 Market St', city: 'San Francisco', state: 'CA', postalCode: '94105', country: 'US' };
        const r = await mod('POST', '/checkout', '/', { cartId: cid, provider: 'stripe', shippingAddress: addr });
        cartId = null; // start a fresh cart after an attempt
        return new Response(orderPage(r), { headers: { 'content-type': 'text/html' } });
      }
      // ---- static site ----
      let rel = p === '/' ? '/index.html' : p;
      if (!extname(rel)) {
        const cand = join(siteRoot, rel.replace(/\/$/, '') + '.html');
        if (existsSync(cand)) rel = rel.replace(/\/$/, '') + '.html';
      }
      const r = serveFile(join(siteRoot, rel));
      return r ?? new Response('not found', { status: 404 });
    } catch (e: any) {
      return new Response('error: ' + (e?.message || e), { status: 500 });
    }
  },
});

// Present the module cart in Shopify's cart.js shape so the captured theme is happy.
async function shopifyCart() {
  const cid = cartId; if (!cid) return { item_count: 0, items: [], total_price: 0 };
  const r = await mod('GET', `/cart/${cid}`, `/${cid}`);
  const items = (r.body?.items || []).map((it: any) => ({
    quantity: it.quantity, title: it.variantTitle || it.title, price: it.unitPriceCents ?? it.priceCents ?? 0,
  }));
  const count = items.reduce((s: number, i: any) => s + i.quantity, 0);
  const total = items.reduce((s: number, i: any) => s + i.price * i.quantity, 0);
  return { token: 'fl-cart', item_count: count, total_price: total, currency: 'USD', items };
}

function orderPage(r: { code: number; body: any }): string {
  const ok = r.code === 200;
  const order = r.body?.order || r.body;
  const created = r.code === 200 || r.code === 502; // 502 = order created, payment (Stripe) not initiated
  const status = order?.status || (r.code === 502 ? 'created (payment pending — Stripe key needed)' : 'error');
  const oid = order?.id || (typeof r.body === 'string' ? (r.body.match(/[0-9a-f-]{36}/)?.[0] ?? '') : '');
  const total = order?.totalCents != null ? money(order.totalCents) : '';
  return `<!doctype html><html><head><meta charset=utf-8><title>Order — ForkLaunch</title></head>
<body style="font:16px/1.6 system-ui;max-width:640px;margin:60px auto;padding:0 24px;color:#111">
  <div style="width:64px;height:64px;border-radius:50%;background:${created ? '#111' : '#c00'};color:#fff;font-size:32px;line-height:64px;text-align:center;margin:0 auto 20px">${created ? '✓' : '×'}</div>
  <h1 style="text-align:center">${created ? 'Order placed on ForkLaunch' : 'Checkout error'}</h1>
  <div style="background:#fafafa;border:1px solid #eee;border-radius:12px;padding:20px;margin-top:24px">
    <div><b>Order</b>: ${oid || '—'}</div>
    <div><b>Status</b>: ${status}</div>
    ${total ? `<div><b>Total</b>: ${total}</div>` : ''}
    <div style="margin-top:10px;color:#888;font-size:13px">Backed by the ForkLaunch ecommerce module${ok ? '' : ' — add a real STRIPE_API_KEY to complete the card charge'}.</div>
  </div>
  <p style="text-align:center;margin-top:24px"><a href="/">← Continue shopping</a></p>
</body></html>`;
}

console.log(`storefront (wired to ForkLaunch module ${MODULE}) on http://localhost:${PORT}`);
