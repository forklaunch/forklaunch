/**
 * Merch storefront BFF.
 *
 * The ecommerce module's catalog routes are access:'internal' and HMAC-signed,
 * so the browser can never call them directly — this process holds the secret,
 * signs on the server side, and exposes a small public read surface plus
 * cart/checkout passthrough. Same shape a real storefront would use.
 */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { generateHmacAuthHeaders } from '@forklaunch/core/http';

const API = process.env.ECOM_API ?? 'http://localhost:8020';
const SECRET = process.env.HMAC_SECRET_KEY;
const PORT = Number(process.env.PORT ?? 4310);
// Gate for the orders view. Absent means the view is off rather than open —
// failing closed, because the failure mode of the alternative is publishing
// customer addresses.
const ADMIN_TOKEN = process.env.ADMIN_TOKEN ?? '';

/** Constant-time compare, so a wrong token leaks nothing through timing. */
function timingSafeEqual(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}
if (!SECRET) { console.error('HMAC_SECRET_KEY required'); process.exit(1); }

async function api(method, route, signPath, body) {
  const payload = body === undefined ? undefined : JSON.stringify(body);
  const { authorization } = generateHmacAuthHeaders({ secretKey: SECRET, method, path: signPath, body: payload });
  const r = await fetch(API + route, {
    method, headers: { 'Content-Type': 'application/json', Authorization: authorization },
    ...(payload !== undefined ? { body: payload } : {})
  });
  const text = await r.text();
  let parsed = null;
  try { parsed = text ? JSON.parse(text) : null; }
  catch { parsed = { raw: text.slice(0, 200) }; }   // module 500s are plain text
  return { status: r.status, body: parsed };
}

const MIME = { '.html':'text/html', '.css':'text/css', '.js':'text/javascript', '.svg':'image/svg+xml' };

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://localhost:${PORT}`);
  const send = (code, obj) => { res.writeHead(code, {'Content-Type':'application/json'}); res.end(JSON.stringify(obj)); };

  try {
    // ---- public read surface -------------------------------------------
    if (url.pathname === '/shop/products') {
      const { status, body } = await api('GET', '/product', '/');
      if (status !== 200) return send(status, { error: 'catalog unavailable' });
      const withVariants = await Promise.all(body.map(async (p) => {
        const v = await api('GET', `/variant?productId=${p.id}`, '/');
        const variants = (v.status === 200 ? v.body : []).filter(x => x.productId === p.id);
        const stock = await Promise.all(variants.map(async (vr) => {
          const s = await api('GET', `/inventory/${vr.id}`, `/${vr.id}`);
          return [vr.id, s.status === 200 ? s.body.stock : 0];
        }));
        const stockMap = Object.fromEntries(stock);
        return { ...p, variants: variants.map(vr => ({ ...vr, stock: stockMap[vr.id] ?? 0 })) };
      }));
      return send(200, withVariants);
    }

    // ---- real cart, backed by the module --------------------------------
    // POST /shop/cart            -> creates a cart, returns { id }
    // POST /shop/cart/items      -> { cartId, variantId, quantity }
    // GET  /shop/cart/:id        -> the cart as the module stores it
    if (url.pathname === '/shop/cart' && req.method === 'POST') {
      const out = await api('POST', '/cart', '/', {});
      return send(out.status, out.body);
    }
    if (url.pathname === '/shop/cart/items' && req.method === 'POST') {
      const chunks = []; for await (const c of req) chunks.push(c);
      const body = JSON.parse(Buffer.concat(chunks).toString() || '{}');
      const out = await api('POST', '/cart/items', '/items', body);
      return send(out.status, out.body);
    }
    // Remove one line from the cart. The module signs this route relative to
    // its own mount, so the signed path drops the /shop prefix and the /cart
    // segment the router already owns.
    if (url.pathname.startsWith('/shop/cart/') && req.method === 'DELETE') {
      const parts = url.pathname.split('/').filter(Boolean);   // shop cart :id items :variantId
      if (parts.length === 5 && parts[3] === 'items') {
        const [, , cartId, , variantId] = parts;
        const out = await api('DELETE', `/cart/${cartId}/items/${variantId}`, `/${cartId}/items/${variantId}`);
        return send(out.status, out.body);
      }
    }

    if (url.pathname.startsWith('/shop/cart/') && req.method === 'GET') {
      const id = url.pathname.split('/').pop();
      const out = await api('GET', `/cart/${id}`, `/${id}`);
      return send(out.status, out.body);
    }

    // ---- checkout passthrough -------------------------------------------
    if (url.pathname.startsWith('/shop/checkout')) {
      const chunks = []; for await (const c of req) chunks.push(c);
      const raw = Buffer.concat(chunks).toString() || undefined;
      const body = raw ? JSON.parse(raw) : undefined;
      const target = url.pathname.replace('/shop', '');
      const signPath = target.startsWith('/cart/') ? '/' + target.split('/').slice(2).join('/') : '/';
      const out = await api(req.method, target, signPath || '/', body);
      return send(out.status, out.body);
    }

    // Orders, for the shop-owner view. Read-only: the page shows what the
    // module recorded and cannot change it, so nothing here needs the write
    // side of the order API.
    if (url.pathname === '/shop/orders') {
      // Orders carry customer names and shipping addresses. Without a gate
      // this endpoint hands every one of them to anyone who can reach the
      // port, which on a deployed store is a straightforward data leak.
      //
      // A shared token is the least this can be and still be defensible: it
      // is not a login, there are no accounts, and it does not belong in
      // front of anything but a demo. Compared per-character rather than with
      // === to keep the comparison time-independent, so the token cannot be
      // guessed a character at a time from response timing.
      if (!ADMIN_TOKEN) {
        return send(503, { error: 'orders view is disabled: set ADMIN_TOKEN' });
      }
      const offered =
        (req.headers['authorization'] || '').replace(/^Bearer /i, '') ||
        url.searchParams.get('key') ||
        '';
      if (!timingSafeEqual(offered, ADMIN_TOKEN)) {
        return send(401, { error: 'unauthorized' });
      }
      const { status, body } = await api('GET', '/order', '/');
      if (status !== 200) return send(status, { error: 'orders unavailable' });
      // Newest first — an order list is read from the top.
      const orders = Array.isArray(body)
        ? [...body].sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt)))
        : [];
      return send(200, orders);
    }

    // Publishable key for Stripe.js. Safe to expose — it can only create
    // payment methods, never move money or read the account.
    if (url.pathname === '/shop/config') {
      return send(200, { publishableKey: process.env.STRIPE_PUBLISHABLE_KEY ?? null });
    }

    // ---- static ----------------------------------------------------------
    const file = url.pathname === '/' ? '/index.html'
               : url.pathname === '/checkout' ? '/checkout.html'
               : url.pathname === '/orders' ? '/orders.html'
               : url.pathname === '/bag' ? '/bag.html'
               // Every product URL serves the same page; the handle in the path
               // is read client-side, the way a single-page shop routes.
               : url.pathname.startsWith('/products/') ? '/product.html'
               : url.pathname;
    const fp = path.join(process.cwd(), 'public', file);
    if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
      res.writeHead(200, {'Content-Type': MIME[path.extname(fp)] ?? 'application/octet-stream'});
      return res.end(fs.readFileSync(fp));
    }
    res.writeHead(404); res.end('not found');
  } catch (e) {
    send(500, { error: String(e.message ?? e) });
  }
});

server.listen(PORT, () => console.log(`storefront on http://localhost:${PORT}`));
