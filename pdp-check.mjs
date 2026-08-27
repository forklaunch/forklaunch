import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
const S = 'http://localhost:4310';
const PG = 'postgresql://postgresql@localhost:5434/forklaunch-supply-ecommerce';
const sql = q => execFileSync('psql', [PG, '-tAc', q], { encoding: 'utf8' }).trim();
const out = [];
const check = (n, p, d='') => out.push({n, p: !!p, d});

const b = await chromium.launch();
const page = await (await b.newContext()).newPage();
try {
  // shop card links to the PDP
  await page.goto(S + '/', { waitUntil: 'load' });
  await page.waitForTimeout(2500);
  const href = await page.evaluate(() => document.querySelector('.card a.thumb')?.getAttribute('href'));
  check('shop card links to a product page', href?.startsWith('/products/'), href);

  await page.click('.card a.thumb');
  await page.waitForTimeout(2500);
  const v = await page.evaluate(() => ({
    url: location.pathname,
    h1: document.querySelector('h1')?.textContent.trim(),
    price: document.querySelector('.price')?.textContent.trim(),
    stock: document.querySelector('.stock')?.textContent.trim(),
    opts: [...document.querySelectorAll('.opt')].map(o => o.textContent.trim()),
    art: !!document.querySelector('.stage svg'),
    crumbs: document.getElementById('crumbs')?.textContent.replace(/\s+/g,' ').trim()
  }));
  check('product page renders', v.h1 && v.art && /^\$\d/.test(v.price), `${v.url} · ${v.h1} · ${v.price}`);
  check('breadcrumbs present', /Shop \/ .+ \/ /.test(v.crumbs), v.crumbs);
  check('variant options render', v.opts.length > 1, v.opts.join(' '));
  check('live stock shown', /in stock|Only \d+ left|Out of stock/.test(v.stock), v.stock);

  // switching size changes what the page says
  const before = v.price + '|' + v.stock;
  await page.evaluate(() => document.querySelectorAll('.opt')[2]?.click());
  await page.waitForTimeout(400);
  const after = await page.evaluate(() => ({
    stock: document.querySelector('.stock')?.textContent.trim(),
    price: document.querySelector('.price')?.textContent.trim(),
    picked: document.querySelector('.opt[aria-pressed="true"]')?.textContent.trim(),
    label: document.querySelector('.lbl em')?.textContent.trim()
  }));
  check('size selection drives the page', after.picked === after.label && after.picked === v.opts[2],
        `picked ${after.picked}, label says ${after.label}, stock ${after.stock}`);

  // add to bag reaches the module
  await page.evaluate(() => localStorage.removeItem('fl_cart'));
  await page.reload({ waitUntil: 'load' });
  await page.waitForTimeout(2200);
  const size = await page.evaluate(() => { document.querySelectorAll('.opt')[1]?.click(); return document.querySelector('.opt[aria-pressed="true"]').textContent.trim(); });
  await page.click('#add');
  await page.waitForTimeout(3000);
  const cart = await page.evaluate(() => localStorage.getItem('fl_cart'));
  const lines = cart ? Number(sql(`select coalesce(jsonb_array_length(items),0) from cart where id='${cart}';`) || 0) : 0;
  check('add to bag reaches the module', lines > 0, `cart ${String(cart).slice(0,8)} holds ${lines} line(s) server-side`);

  const chrome = await page.evaluate(() => ({
    count: document.getElementById('count')?.textContent,
    toast: document.querySelector('.toast.on')?.textContent.replace(/\s+/g,' ').trim(),
    btn: document.getElementById('add')?.textContent.trim()
  }));
  check('header bag count updates', chrome.count === '1', 'count = ' + chrome.count);
  check('confirmation offers the bag', /View bag/.test(chrome.toast || ''), chrome.toast);

  // the size added is the size chosen, not the default
  const wantVariant = sql(`select v.id from variant v join product p on p.id::text=v.product_id::text where p.handle='tee-flame' and v.option_values->>'Size'='${size}';`);
  const inCart = sql(`select (i->>'variantId') from cart c cross join lateral jsonb_array_elements(c.items) i where c.id='${cart}' limit 1;`);
  check('the chosen size is what was added', wantVariant && wantVariant === inCart, `size ${size}`);

  // bag page agrees
  await page.goto(S + '/bag', { waitUntil: 'load' });
  await page.waitForTimeout(2500);
  // Read the rendered lines, not body.textContent — that includes the inline
  // script source, so any phrase the page can print is always "present".
  const bag = await page.evaluate(() => ({
    titles: [...document.querySelectorAll('.line .info h3')].map(h => h.textContent.trim()),
    empty: !!document.querySelector('.empty')
  }));
  check('the bag shows what the product page added',
        !bag.empty && bag.titles.includes('Flame Tee'), bag.titles.join(', ') || 'empty');

  // unknown handle
  await page.goto(S + '/products/does-not-exist', { waitUntil: 'load' });
  await page.waitForTimeout(2000);
  const nf = await page.evaluate(() => document.querySelector('.missing h1')?.textContent.trim());
  check('unknown product says so instead of blanking', nf === 'Not found', nf);

  const errs = [];
  page.on('pageerror', e => errs.push(e.message));
  await page.goto(S + '/products/hoodie-cascade', { waitUntil: 'load' });
  await page.waitForTimeout(2000);
  check('no page errors', errs.length === 0, errs.join('; ') || 'clean');
} finally { await b.close(); }

console.log('\nPDP check — ' + S + '\n');
for (const r of out) console.log(`  ${r.p ? 'PASS' : 'FAIL'}  ${r.n}${r.d ? '   (' + r.d + ')' : ''}`);
const f = out.filter(r => !r.p).length;
console.log(`\n${out.length - f}/${out.length} passed\n`);
process.exit(f ? 1 : 0);
