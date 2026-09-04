const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch({headless:true});
  const p = await b.newPage({viewport:{width:1440,height:900}});
  // 1. does the cart API work at all?
  await p.goto('http://127.0.0.1:8090/index.html',{waitUntil:'domcontentloaded'});
  await p.waitForTimeout(3000);
  console.log('  bridge mode:', await p.evaluate(()=>window.__forklaunchBridge?.mode||'ABSENT'));
  const api = await p.evaluate(async()=>{
    const a=await fetch('/cart/add.js',{method:'POST',headers:{'Content-Type':'application/json'},
      body:JSON.stringify({id:1,quantity:1,title:'T',price:1000})}).then(r=>r.json());
    return a.item_count;
  }).catch(e=>'ERR');
  console.log('  cart API (fetch):', api ? `works, count=${api}` : 'failed');

  // 2. what happens on a real PDP when you click through?
  const fs=require('fs');
  const prods=fs.readdirSync('/Users/family/projects/forklaunch-work/demos/allbirds-v5/site/products');
  await p.goto('http://127.0.0.1:8090/products/'+prods[0],{waitUntil:'domcontentloaded'});
  await p.waitForTimeout(3500);
  const pdp = await p.evaluate(()=>{
    const buy=[...document.querySelectorAll('button,a')].filter(e=>{
      const t=(e.textContent||'').trim(); const r=e.getBoundingClientRect();
      return /add to (cart|bag)|select a size|buy/i.test(t) && r.width>40 && getComputedStyle(e).display!=='none';
    }).map(e=>(e.textContent||'').trim().slice(0,22));
    const sizes=[...document.querySelectorAll('button,li,label')].filter(e=>
      /^(\d{1,2}(\.5)?|XS|S|M|L|XL)$/i.test((e.textContent||'').trim()) && e.getBoundingClientRect().width>15).length;
    return {visibleBuyControls:buy, sizeOptions:sizes};
  });
  console.log('  PDP buy controls:', JSON.stringify(pdp));

  // 3. filters on a collection page
  await p.goto('http://127.0.0.1:8090/collections/men.html',{waitUntil:'domcontentloaded'});
  await p.waitForTimeout(3000);
  const filt = await p.evaluate(()=>{
    const f=[...document.querySelectorAll('button,select,input[type=checkbox]')].filter(e=>{
      const t=(e.textContent||e.getAttribute('aria-label')||'').toLowerCase();
      return /filter|sort|size|color|refine/.test(t);
    });
    return {filterControls:f.length, labels:f.slice(0,4).map(e=>(e.textContent||e.getAttribute('aria-label')||'').trim().slice(0,20))};
  });
  console.log('  filters present:', JSON.stringify(filt));
  await b.close();
})();
