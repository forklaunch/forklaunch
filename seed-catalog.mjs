/**
 * Seeds the demo catalog. Products and variants go through the module's own
 * HMAC-signed API. Inventory is written directly: the module exposes only
 * GET /:variantId, PUT /adjust and POST /check — there is no create-inventory
 * endpoint, and /adjust throws on a variant with no row yet. That gap is
 * logged as a real API bug; seeding around it here.
 */
import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { execFileSync } from 'node:child_process';

const API = process.env.ECOM_API ?? 'http://localhost:8020';
const SECRET = process.env.HMAC_SECRET_KEY;
if (!SECRET) { console.error('HMAC_SECRET_KEY required'); process.exit(1); }

async function call(method, route, signPath, body) {
  const payload = body === undefined ? undefined : JSON.stringify(body);
  const { authorization } = generateHmacAuthHeaders({ secretKey: SECRET, method, path: signPath, body: payload });
  const r = await fetch(API + route, { method,
    headers: { 'Content-Type': 'application/json', Authorization: authorization },
    ...(payload !== undefined ? { body: payload } : {}) });
  const t = await r.text();
  if (!r.ok) throw new Error(`${method} ${route} -> ${r.status} ${t.slice(0,200)}`);
  return t ? JSON.parse(t) : null;
}
const PG = (process.env.PG_EXEC ?? 'psql -d fl_merch_demo').split(' ');
const sql = (q) => execFileSync(PG[0], [...PG.slice(1), '-tAc', q]).toString().trim();

const APPAREL = ['XS','S','M','L','XL','2XL'];
const CATALOG = [
  // ---- apparel ----------------------------------------------------------
  { h:'tee-flame',    t:'Flame Tee',            ty:'Apparel', p:3200, d:'Heavyweight 240gsm cotton. Flame mark across the chest.', o:APPAREL, stock:42 },
  { h:'tee-mono',     t:'Monospace Tee',        ty:'Apparel', p:3200, d:'Garment-dyed, washed soft. `forklaunch` set in Plex Mono.', o:APPAREL, stock:38 },
  { h:'tee-stack',    t:'Stack Tee — Charcoal', ty:'Apparel', p:3400, d:'The stack diagram, screen-printed in three passes.', o:APPAREL, stock:26 },
  { h:'hoodie-cascade', t:'Cascade Hoodie',     ty:'Apparel', p:6800, d:'Midweight fleece, embroidered cascade on the sleeve.', o:APPAREL, stock:25, was:7800 },
  { h:'crew-ember',   t:'Ember Crewneck',       ty:'Apparel', p:5800, d:'Boxy fit, brushed inside, ribbed cuffs.', o:APPAREL, stock:31 },
  { h:'cap-six',      t:'Six-Panel Cap',        ty:'Apparel', p:2800, d:'Structured six-panel, low-profile, brass slider.', o:['One Size'], stock:64 },
  { h:'beanie-cuff',  t:'Cuffed Beanie',        ty:'Apparel', p:2600, d:'Rib-knit merino blend, folded cuff, woven label.', o:['One Size'], stock:48 },
  // ---- desk -------------------------------------------------------------
  { h:'keycaps',      t:'Keycap Set — Flame',   ty:'Desk',    p:4500, d:'Six PBT doubleshot keycaps. Cherry profile, esc + arrows + fn.', o:['Cherry','MX'], stock:33 },
  { h:'deskmat',      t:'Desk Mat — Cascade',   ty:'Desk',    p:3900, d:'900×400mm stitched-edge mat. Cascade gradient, matte weave.', o:['900×400'], stock:29 },
  { h:'notebook',     t:'Dot Grid Notebook',    ty:'Desk',    p:1900, d:'A5, 160gsm dot grid, lay-flat binding, flame foil.', o:['A5'], stock:75 },
  { h:'mug-deploy',   t:'Deploy Mug',           ty:'Desk',    p:1800, d:'15oz ceramic. Holds enough for one incident.', o:['15oz'], stock:90 },
  { h:'bottle',       t:'Insulated Bottle',     ty:'Desk',    p:3400, d:'620ml double-wall steel, powder coat, laser mark.', o:['620ml'], stock:44 },
  // ---- small goods -------------------------------------------------------
  { h:'stickers',     t:'Sticker Pack',         ty:'Small goods', p:900,  d:'Six die-cut vinyl stickers. Laptop tax, paid.', o:['Pack of 6'], stock:300 },
  { h:'pins',         t:'Enamel Pin Set',       ty:'Small goods', p:1600, d:'Three hard-enamel pins, gold plating, rubber backs.', o:['Set of 3'], stock:82 },
  { h:'socks',        t:'Monorepo Socks',       ty:'Small goods', p:1400, d:'Ribbed crew socks. One repo, two socks.', o:['M','L'], stock:60 },
  { h:'tote',         t:'Canvas Tote',          ty:'Small goods', p:2200, d:'12oz natural canvas, boxed corners, webbed handles.', o:['One Size'], stock:55 },
  { h:'lanyard',      t:'Woven Lanyard',        ty:'Small goods', p:1200, d:'Jacquard-woven, breakaway clasp, matte hardware.', o:['One Size'], stock:120 }
];

const existing = new Set(sql("select handle from product").split('\n').filter(Boolean));
let made = 0, skipped = 0;
for (const it of CATALOG) {
  if (existing.has(it.h)) { skipped++; continue; }
  const prod = await call('POST','/product','/', {
    externalId: 'fl-' + it.h, handle: it.h, title: it.t, description: it.d, status: 'ACTIVE'
  });
  sql(`update product set product_type='${it.ty}', description_html='${it.d.replace(/'/g,"''")}' where id='${prod.id}'`);
  for (const o of it.o) {
    const v = await call('POST','/variant','/', {
      productId: prod.id, externalId: `fl-${it.h}-${String(o).toLowerCase().replace(/[^a-z0-9]/g,'')}`, title: `${it.t} — ${o}`, sku: `${it.h}-${String(o).toLowerCase().replace(/[^a-z0-9]/g,'')}`,
      priceCents: it.p, currency: 'usd', optionValues: { Size: o }, requiresShipping: true
    });
    if (it.was) sql(`update variant set compare_at_price_cents=${it.was} where id='${v.id}'`);
    const per = Math.max(0, Math.round(it.stock / it.o.length) + (o==='M'||o==='L' ? 8 : 0));
    sql(`insert into inventory (id, created_at, updated_at, variant_id, stock)
         values (gen_random_uuid(), now(), now(), '${v.id}', ${per})`);
  }
  made++; process.stdout.write(`  + ${it.t} (${it.o.length})\n`);
}
console.log(`\nseeded ${made}, skipped ${skipped} existing`);
