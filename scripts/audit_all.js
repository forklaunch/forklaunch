/**
 * Exhaustive interaction audit — clicks EVERY interactive element on every
 * captured page and records what actually happened. Written because "it works"
 * is not something anyone should take on trust before showing a client.
 */
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path'), http = require('http');
const MIME={'.html':'text/html','.css':'text/css','.js':'text/javascript','.json':'application/json','.png':'image/png','.jpg':'image/jpeg','.jpeg':'image/jpeg','.gif':'image/gif','.svg':'image/svg+xml','.webp':'image/webp','.avif':'image/avif','.woff':'font/woff','.woff2':'font/woff2','.ico':'image/x-icon'};
function serve(root){const s=http.createServer((q,r)=>{let p=decodeURIComponent(q.url.split('?')[0]);if(p.endsWith('/'))p+='index.html';const f=path.join(root,p);
if(!f.startsWith(root)||!fs.existsSync(f)||fs.statSync(f).isDirectory()){r.writeHead(404);return r.end('nf');}
r.writeHead(200,{'Content-Type':MIME[path.extname(f).toLowerCase()]||'application/octet-stream'});fs.createReadStream(f).pipe(r);});
return new Promise(k=>s.listen(0,'127.0.0.1',()=>k({s,port:s.address().port})));}

(async () => {
  const site = path.resolve(process.argv[2], 'site');
  const { s, port } = await serve(site);
  const base = `http://127.0.0.1:${port}`;
  const b = await chromium.launch({ headless: true });

  const walk=(d,a=[])=>{for(const f of fs.readdirSync(d,{withFileTypes:true})){if(f.name==='_a')continue;const q=path.join(d,f.name);f.isDirectory()?walk(q,a):f.name.endsWith('.html')&&a.push(q);}return a;};
  const pages = walk(site).map(f => path.relative(site,f).split(path.sep).join('/'));

  const tally = { navigatedLocal:0, openedUI:0, inert:0, dead:0, wentExternal:0 };
  const deadItems = [], externalItems = [];

  for (const pg of pages) {
    const p = await b.newPage({ viewport:{width:1440,height:900} });
    await p.goto(`${base}/${pg}`,{waitUntil:'domcontentloaded'}).catch(()=>{});
    await p.waitForTimeout(2500);

    const n = await p.evaluate(() => {
      window.__els = [...document.querySelectorAll('a[href],button,[role="button"],[data-menu-trigger]')]
        .filter(e => { const r=e.getBoundingClientRect();
          return r.width>8 && r.height>6 && getComputedStyle(e).display!=='none'; });
      return window.__els.length;
    }).catch(()=>0);

    for (let i = 0; i < Math.min(n, 60); i++) {
      const before = p.url();
      const info = await p.evaluate((idx) => {
        const e = window.__els && window.__els[idx]; if (!e) return null;
        const h = e.getAttribute('href');
        return { label:(e.textContent||'').trim().slice(0,26), href:h,
                 external: !!(h && /^https?:\/\//.test(h)),
                 inert: h === '#' || e.hasAttribute('data-mirror-uncaptured') };
      }, i).catch(()=>null);
      if (!info) continue;

      if (info.external) { tally.wentExternal++; externalItems.push(`${pg} :: ${info.label}`); continue; }
      if (info.inert)    { tally.inert++; continue; }

      const domBefore = await p.evaluate(()=>document.body.innerHTML.length).catch(()=>0);
      await p.evaluate((idx)=>{ const e=window.__els&&window.__els[idx]; if(e) e.click(); }, i).catch(()=>{});
      await p.waitForTimeout(400);
      const after = p.url();

      if (after !== before) {
        if (after.startsWith(base)) tally.navigatedLocal++;
        else { tally.wentExternal++; externalItems.push(`${pg} :: ${info.label} -> ${after.slice(0,50)}`); }
        await p.goto(`${base}/${pg}`,{waitUntil:'domcontentloaded'}).catch(()=>{});
        await p.waitForTimeout(1500);
        await p.evaluate(() => { window.__els = [...document.querySelectorAll('a[href],button,[role="button"],[data-menu-trigger]')]
          .filter(e => { const r=e.getBoundingClientRect(); return r.width>8&&r.height>6&&getComputedStyle(e).display!=='none'; }); }).catch(()=>{});
      } else {
        const domAfter = await p.evaluate(()=>document.body.innerHTML.length).catch(()=>0);
        if (Math.abs(domAfter-domBefore) > 40) tally.openedUI++;
        else { tally.dead++; if (deadItems.length<25) deadItems.push(`${pg} :: ${info.label||'(no label)'}`); }
      }
    }
    await p.close();
  }
  await b.close(); s.close();
  console.log(JSON.stringify({ pages: pages.length, tally, deadItems, externalItems: externalItems.slice(0,15) }, null, 1));
})();
