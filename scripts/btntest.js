const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch({headless:true});
  const p = await b.newPage({viewport:{width:1440,height:900}});
  for (const lbl of ['Shop Mens','Shop Womens','Shop Socks']) {
    await p.goto('http://127.0.0.1:8090/collections/mens-bestsellers.html',{waitUntil:'domcontentloaded'});
    await p.waitForTimeout(3000);
    const before = p.url();
    const el = await p.evaluateHandle((l)=>[...document.querySelectorAll('button')]
      .find(e=>(e.textContent||'').trim()===l)||null, lbl);
    if (!await el.evaluate(e=>!!e).catch(()=>false)) { console.log(`  ${lbl}: not present`); continue; }
    await el.evaluate(e=>e.click()).catch(()=>{});
    await p.waitForTimeout(1500);
    const moved = p.url()!==before;
    console.log(`  ${lbl.padEnd(12)}: ${moved ? 'WORKS -> '+p.url().split('/').slice(-1)[0] : 'still dead'}`);
  }
  await b.close();
})();
