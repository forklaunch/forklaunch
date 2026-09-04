const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch({headless:true});
  const p = await b.newPage({viewport:{width:1440,height:900}});
  for (const label of ['Men','Women','Sale','New Arrivals','Shop All']) {
    await p.goto('http://127.0.0.1:8090/index.html',{waitUntil:'domcontentloaded'});
    await p.waitForTimeout(3000);
    const start = p.url();
    const clicked = await p.evaluate((lab) => {
      const el = [...document.querySelectorAll('a,button,[role=button]')]
        .find(e => (e.textContent||'').trim().toLowerCase() === lab.toLowerCase()
                   && e.getBoundingClientRect().top < 340
                   && e.getBoundingClientRect().width > 10);
      if (!el) return 'NOT FOUND';
      el.click(); return 'clicked';
    }, label);
    await p.waitForTimeout(2200);
    const moved = p.url() !== start;
    const dest = p.url().split('/').slice(-1)[0] || '(root)';
    console.log(`  ${label.padEnd(14)} ${clicked === 'NOT FOUND' ? 'NOT FOUND' : (moved ? '-> ' + dest : 'NO NAVIGATION')}`);
  }
  await b.close();
})();
