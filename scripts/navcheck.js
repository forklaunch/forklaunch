const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch({headless:true});
  const p = await b.newPage({viewport:{width:1440,height:900}});
  for (const label of ['Men','Women','Sale']) {
    await p.goto('http://127.0.0.1:8090/index.html',{waitUntil:'domcontentloaded'});
    await p.waitForTimeout(3500);
    const before = p.url();
    const el = await p.evaluateHandle((lbl) => {
      return [...document.querySelectorAll('button,a')].find(e => {
        const t=(e.textContent||'').trim();
        const r=e.getBoundingClientRect();
        return t.toLowerCase()===lbl.toLowerCase() && r.top<340 && r.width>10;
      }) || null;
    }, label);
    const found = await el.evaluate(e=>!!e).catch(()=>false);
    if (!found) { console.log(`  ${label.padEnd(6)} : NOT FOUND in nav`); continue; }
    await el.asElement().click({timeout:5000}).catch(()=>{});
    await p.waitForTimeout(2000);
    const moved = p.url() !== before;
    const dest = p.url().replace('http://127.0.0.1:8090/','');
    console.log(`  ${label.padEnd(6)} : ${moved ? 'WORKS -> '+dest : 'did nothing'}`);
  }
  await b.close();
})();
