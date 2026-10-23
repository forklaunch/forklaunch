#!/usr/bin/env bun
/**
 * heroserve-pg — the united storefront on a real local Postgres.
 *
 *   bun scripts/heroserve-pg.ts <slug> <siteRoot> <port> <pgUrl>
 *
 * Same as heroserve.ts but the full purchase flow runs against Postgres (via
 * PgStore) instead of SQLite. Pixel-faithful captured pages for the look; the
 * Postgres scaffold (a stand-in mirroring the module, NOT its code) for the
 * function: add-to-cart, cart, checkout, order state machine, pay, inventory.
 */
import { existsSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { PgStore } from './pgstore.ts';
import { orderView } from './views.ts';
import type { Brand } from './brand.ts';

const slug = process.argv[2];
const siteRoot = process.argv[3];
const SITE_ROOT_ABS = realpathSync(siteRoot);
// Request-derived paths stay inside the capture, after symlinks.
function inside(fp: string): string | null {
  const abs = resolve(fp);
  if (abs !== SITE_ROOT_ABS && !abs.startsWith(SITE_ROOT_ABS + sep)) return null;
  try { const real = realpathSync(abs); return real === SITE_ROOT_ABS || real.startsWith(SITE_ROOT_ABS + sep) ? real : null; } catch { return null; }
}
const esc = (t: unknown) => String(t ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c] as string));
const port = Number(process.argv[4] ?? 4600);
const pgUrl = process.argv[5] ?? 'postgres://localhost/forklaunch_migration_scaffold';

const store = await new PgStore(pgUrl).init();
const brandPath = join('data', slug, 'brand.json');
const brand: Brand | null = existsSync(brandPath) ? JSON.parse(readFileSync(brandPath, 'utf8')) : null;

let demoCart: number | null = null;
async function cart() { if (demoCart == null) demoCart = await store.createCart(); return demoCart; }

async function shopifyCart() {
  const c = await store.getCart(await cart());
  const items = await Promise.all(c.items.map(async (it: any) => ({
    id: Number(await store.variantExternalById(it.variant_id)), quantity: it.quantity,
    key: `${it.variant_id}:${it.id}`, title: `${it.product_title} - ${it.variant_title}`,
    product_title: it.product_title, variant_title: it.variant_title, price: it.price_cents,
    line_price: it.price_cents * it.quantity, original_line_price: it.price_cents * it.quantity,
    handle: it.handle, requires_shipping: true, url: `/products/${it.handle}`,
  })));
  const count = items.reduce((n, it) => n + it.quantity, 0);
  return { token: 'fl-cart', note: '', attributes: {}, item_count: count, total_price: c.subtotalCents,
    total_discount: 0, original_total_price: c.subtotalCents, items_subtotal_price: c.subtotalCents,
    currency: 'USD', requires_shipping: true, items };
}

const SHIM = `<script>(function(){
  var of=window.fetch;
  function upd(n){try{document.querySelectorAll('.cart-count-bubble,[data-cart-count],.cart-count,#cart-icon-bubble span,.header__cart-count').forEach(function(e){var s=e.querySelector('span')||e;s.textContent=n;e.style.display='';});}catch(e){}}
  function toast(t){var d=document.getElementById('__fl_toast');if(!d){d=document.createElement('div');d.id='__fl_toast';d.style.cssText='position:fixed;top:70px;right:20px;z-index:2147483647;background:#111;color:#fff;padding:14px 18px;border-radius:8px;font-family:system-ui;font-size:14px;box-shadow:0 6px 24px rgba(0,0,0,.4);opacity:0;transition:opacity .2s;max-width:280px';document.body.appendChild(d);}d.innerHTML=t;d.style.opacity='1';clearTimeout(d._t);d._t=setTimeout(function(){d.style.opacity='0';},2600);}
  function refresh(){of('/cart.js').then(function(r){return r.json();}).then(function(c){upd(c.item_count);});}
  window.fetch=async function(i,init){
    var u=(typeof i==='string')?i:(i&&i.url)||'';
    try{
      if(/\\/cart\\/add(\\.js)?/.test(u)){
        var b=init&&init.body,vid,qty=1;
        try{if(typeof b==='string'){if(b[0]==='{'){var p=JSON.parse(b);vid=p.id;qty=p.quantity||1;}else{var sp=new URLSearchParams(b);vid=sp.get('id');qty=+(sp.get('quantity')||1);}}else if(b instanceof FormData){vid=b.get('id');qty=+(b.get('quantity')||1);}}catch(e){}
        var rr=await of('/__fl/cart/add',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({variantExternalId:String(vid),quantity:qty})});
        var jj=await rr.json();refresh();toast('\\u2713 Added to cart &nbsp;<a href="/cart" style="color:#8bf;text-decoration:underline">View cart \\u2192</a>');
        return new Response(JSON.stringify(jj.line||{id:vid,quantity:qty}),{status:200,headers:{'content-type':'application/json'}});
      }
      if(/\\/cart(\\.js|\\.json)(\\?|$)/.test(u)){return of('/__fl/cart.js');}
      if(/\\/cart\\/(change|update)(\\.js)?/.test(u)){return of('/__fl/cart.js');}
    }catch(e){}
    return of(i,init);
  };
  function killPopups(){try{document.querySelectorAll('.klaviyo-form,[class*="kl-teaser"],[id*="klaviyo"],#privy-container,.privy-modal,[id*="attentive_overlay"],[class*="popup-overlay"],[role="dialog"][class*="klaviyo"]').forEach(function(e){e.remove();});}catch(e){}}
  killPopups();var _k=setInterval(killPopups,700);setTimeout(function(){clearInterval(_k);},8000);
  document.addEventListener('DOMContentLoaded',refresh);setTimeout(refresh,400);
  document.addEventListener('submit',function(e){var f=e.target;if(f&&f.action&&/\\/cart\\/add/.test(f.action)){e.preventDefault();var fd=new FormData(f);fetch('/cart/add.js',{method:'POST',body:fd});}},true);
  document.addEventListener('click',function(e){var a=e.target.closest&&e.target.closest('[name="checkout"],[href*="/checkout"]');if(a){e.preventDefault();location.href='/checkout';}},true);
})();</script>`;

const html = (s: string) => new Response(s, { headers: { 'content-type': 'text/html' } });
const json = (o: any, s = 200) => new Response(JSON.stringify(o), { status: s, headers: { 'content-type': 'application/json' } });
const redirect = (to: string) => new Response(null, { status: 303, headers: { location: to } });
const CT: Record<string, string> = { css:'text/css', js:'text/javascript', mjs:'text/javascript', json:'application/json', png:'image/png', jpg:'image/jpeg', jpeg:'image/jpeg', gif:'image/gif', webp:'image/webp', svg:'image/svg+xml', avif:'image/avif', woff:'font/woff', woff2:'font/woff2', ttf:'font/ttf', otf:'font/otf', ico:'image/x-icon', html:'text/html' };

function servePage(p: string): Response | null {
  try { if (!existsSync(p) || statSync(p).isDirectory()) return null; } catch { return null; }
  let h = readFileSync(p, 'utf8');
  h = h.replace(/<script>\(function\(\)\{[\s\S]*?\/cart\/add[\s\S]*?\}\)\(\);<\/script>/, '');
  h = /<\/body>/i.test(h) ? h.replace(/<\/body>/i, SHIM + '</body>') : h + SHIM;
  return html(h);
}
function serveAsset(p: string): Response | null {
  try { if (!existsSync(p) || statSync(p).isDirectory()) return null; } catch { return null; }
  const ext = p.split('.').pop()!.toLowerCase();
  return new Response(Bun.file(p), { headers: { 'content-type': CT[ext] ?? 'application/octet-stream', 'cache-control': 'no-store' } });
}

async function cartPage(): Promise<string> {
  const c = await store.getCart(await cart());
  const chrome = existsSync(join(siteRoot, 'index.html')) ? readFileSync(join(siteRoot, 'index.html'), 'utf8') : '';
  const head = (chrome.match(/<head[\s\S]*?<\/head>/i) || ['<head><meta charset=utf8></head>'])[0];
  const header = (chrome.match(/<header[\s\S]*?<\/header>/i) || [''])[0];
  const rows = c.items.map((it: any) => `<tr style="border-bottom:1px solid #ddd"><td style="padding:14px 8px">${esc(it.product_title)} — ${esc(it.variant_title)}</td><td style="text-align:center">${esc(it.quantity)}</td><td style="text-align:right;padding-right:8px">$${(it.price_cents*it.quantity/100).toFixed(2)}</td></tr>`).join('') || '<tr><td style="padding:24px">Your cart is empty.</td></tr>';
  const accent = brand?.primaryColor || '#c0392b';
  return `<!doctype html><html>${head}<body style="margin:0">${header}
  <div style="max-width:820px;margin:40px auto;padding:0 20px;font-family:system-ui">
    <h1 style="font-size:28px">Your cart</h1><table style="width:100%;border-collapse:collapse;margin:20px 0">${rows}</table>
    <div style="text-align:right;font-size:20px;margin:16px 0">Subtotal: <b>$${(c.subtotalCents/100).toFixed(2)}</b></div>
    <form method="POST" action="/checkout" style="text-align:right"><a href="/" style="margin-right:16px;color:#666">← Continue shopping</a>
    <button ${c.items.length?'':'disabled'} style="background:${accent};color:#fff;border:0;padding:14px 34px;font-size:16px;border-radius:6px;cursor:pointer">Checkout →</button></form>
  </div>${SHIM}</body></html>`;
}

Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url); const p = decodeURIComponent(url.pathname); const m = req.method;
    try {
      if (p === '/__fl/cart/add' && m === 'POST') {
        const b = await req.json().catch(() => ({}));
        const vid = await store.variantIdByExternal(String(b.variantExternalId));
        if (vid) await store.addToCart(await cart(), vid, Math.max(1, Number(b.quantity) || 1));
        return json({ ok: !!vid, line: { id: b.variantExternalId, quantity: b.quantity } });
      }
      if ((p === '/__fl/cart.js' || p === '/cart.js' || p === '/cart.json') && m === 'GET') return json(await shopifyCart());
      if (p === '/cart' && m === 'GET') return html(await cartPage());
      if (p === '/checkout' && (m === 'POST' || m === 'GET')) {
        const c = await store.getCart(await cart());
        if (!c.items.length) return redirect('/cart');
        const { orderId } = await store.checkout(await cart()); demoCart = null;
        return redirect(`/orders/${orderId}`);
      }
      if (p.match(/^\/orders\/\d+\/pay$/) && m === 'POST') { await store.pay(Number(p.split('/')[2])); return redirect(`/orders/${p.split('/')[2]}`); }
      if (p.match(/^\/orders\/\d+$/) && m === 'GET') { const o = await store.getOrder(Number(p.split('/')[2])); return o ? html(orderView(slug, o, brand)) : new Response('not found', { status: 404 }); }
      if (p.startsWith('/_a/')) { const fp = inside(join(siteRoot, p)); const r = fp ? serveAsset(fp) : null; return r ?? new Response('', { status: 404 }); }
      if (p === '/' || p === '') { const r = servePage(join(siteRoot, 'index.html')); if (r) return r; }
      const clean = p.replace(/\/$/, '');
      const capPath = inside(join(siteRoot, clean.replace(/^\//, '') + '.html'));
      const cap = capPath ? servePage(capPath) : null;
      if (cap) return cap;
      const home = servePage(join(siteRoot, 'index.html'));
      return home ?? new Response('not found', { status: 404 });
    } catch (e: any) { return new Response(`error: ${e.message}`, { status: 500 }); }
  },
});
const s = await store.stats();
console.log(`[hero-pg:${slug}] storefront on http://localhost:${port} — Postgres scaffold (${s.products} products / ${s.variants} variants)`);
