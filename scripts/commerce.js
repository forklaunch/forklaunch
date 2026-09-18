/**
 * ForkLaunch commerce overlay — the piece that turns a captured storefront
 * into something you can actually shop end-to-end offline: a fixed
 * "Cart (n)" button, a slide-in cart drawer, a full checkout page (email /
 * address / card form with a live order summary), and an order-confirmation
 * screen with a generated order number. It also injects a guaranteed
 * "Add to Cart $price" button on product pages, because a captured store's
 * own buy button is frequently Sold Out or JS-broken once offline (see
 * crawl.js's --clean store-paused neutralizer for exactly that case).
 *
 * Reads/writes cart state in localStorage under `_flc_demo` — decoupled from
 * bridge.js's own `_fl_cart` local-mode cart so a single native add can't be
 * counted twice (once by the theme's intercepted fetch, once by this overlay).
 *
 * HYDRATION-PROOF DESIGN. This block is injected into <head> (via crawl.js's
 * injectIntoHead), NOT before </body>. Body-level inline <script> tags do not
 * survive on React/Hydrogen/Remix SSR storefronts: client hydration reconciles
 * the <body> and the inline script never takes effect (observed on
 * jonesroadbeauty.com — the overlay elements survived but window.FL was
 * undefined and the <script> was absent from the runtime DOM). A <head> inline
 * script instead runs at parse time, before any body hydration, and the
 * `window.FL` global it defines then persists for the life of the page
 * regardless of what hydration does to the DOM.
 *
 * Because the script now lives in <head> (where buttons/divs are invalid and
 * would not render), it BUILDS its own DOM into <body> at DOMContentLoaded,
 * idempotently, and re-asserts it via a MutationObserver so a store whose
 * hydration wipes body children gets the overlay put back. All click handling
 * is delegated on `document` (capture phase), so it works even across DOM
 * teardown/rebuild.
 *
 * Exported as a function (mirrors bridge.js's buildBridge) so crawl.js can
 * inline it per page — never via html.replace's string-replacement form,
 * which corrupts injected scripts containing literal '$' sequences (this one
 * has several, e.g. the '$' price prefix — see injectBeforeBody's docstring).
 */
function buildCommerceOverlay() {
  return COMMERCE_HEAD;
}

const CSS = `
#fl-cart-btn{position:fixed;top:16px;right:16px;z-index:2147483000;background:#111;color:#fff;border:none;border-radius:999px;padding:10px 16px;font:600 13px system-ui;cursor:pointer;box-shadow:0 4px 14px rgba(0,0,0,.25)}
#fl-cart-btn .n{background:#fff;color:#111;border-radius:999px;padding:1px 7px;margin-left:6px;font-weight:700}
#fl-drawer{position:fixed;top:0;right:-420px;width:390px;max-width:92vw;height:100%;background:#fff;z-index:2147483200;box-shadow:-8px 0 30px rgba(0,0,0,.2);transition:right .28s;display:flex;flex-direction:column;font:14px/1.5 system-ui;color:#111}
#fl-drawer.open{right:0}
#fl-ov{position:fixed;inset:0;background:rgba(0,0,0,.4);z-index:2147483100;opacity:0;pointer-events:none;transition:opacity .28s}
#fl-ov.open{opacity:1;pointer-events:auto}
#fl-drawer header{padding:18px 20px;border-bottom:1px solid #eee;display:flex;justify-content:space-between;align-items:center}
#fl-drawer header b{font-size:16px}
#fl-drawer header small{color:#888;font-weight:600;letter-spacing:.04em;text-transform:uppercase;font-size:10px}
#fl-items{flex:1;overflow:auto;padding:8px 20px}
.fl-it{display:flex;gap:12px;padding:12px 0;border-bottom:1px solid #f2f2f2;align-items:center}
.fl-it img{width:56px;height:56px;object-fit:cover;border-radius:8px;background:#f5f5f5}
.fl-it .t{flex:1;font-size:13px}
.fl-it .rm{color:#c00;cursor:pointer;font-size:12px;background:none;border:none}
#fl-foot{padding:18px 20px;border-top:1px solid #eee}
#fl-foot .row{display:flex;justify-content:space-between;font-weight:700;margin-bottom:12px}
#fl-foot button{width:100%;background:#111;color:#fff;border:none;border-radius:10px;padding:14px;font:600 15px system-ui;cursor:pointer}
#fl-empty{padding:40px 20px;text-align:center;color:#999}
.fl-brand{font-size:10px;color:#aaa;text-align:center;padding:8px}
#fl-co{position:fixed;inset:0;z-index:2147483300;background:#fff;overflow:auto;display:none;font:15px/1.6 system-ui;color:#111}
#fl-co.open{display:block}
.fl-co-wrap{max-width:900px;margin:0 auto;padding:40px 24px;display:grid;grid-template-columns:1fr 380px;gap:40px}
@media(max-width:800px){.fl-co-wrap{grid-template-columns:1fr}}
.fl-co-wrap h1{font-size:24px;margin:0 0 4px}
.fl-field{margin:12px 0}
.fl-field label{display:block;font-size:12px;font-weight:600;color:#555;margin-bottom:4px}
.fl-field input{width:100%;padding:11px 12px;border:1px solid #ddd;border-radius:8px;font-size:15px}
.fl-row2{display:grid;grid-template-columns:1fr 1fr;gap:12px}
.fl-summary{background:#fafafa;border:1px solid #eee;border-radius:12px;padding:20px;height:fit-content}
.fl-summary .li{display:flex;justify-content:space-between;padding:8px 0;font-size:14px;border-bottom:1px solid #eee}
.fl-summary .tot{display:flex;justify-content:space-between;padding-top:14px;font-weight:700;font-size:17px}
#fl-place{width:100%;background:#111;color:#fff;border:none;border-radius:10px;padding:15px;font:600 16px system-ui;cursor:pointer;margin-top:16px}
.fl-badge{display:inline-block;background:#eef;color:#334;font-size:11px;font-weight:700;padding:3px 8px;border-radius:6px;margin-bottom:14px}
#fl-done{text-align:center;padding:70px 24px;display:none}
#fl-done.open{display:block}
#fl-done .chk{width:64px;height:64px;border-radius:50%;background:#111;color:#fff;font-size:32px;line-height:64px;margin:0 auto 20px}
#fl-add{position:fixed;left:50%;bottom:24px;transform:translateX(-50%);z-index:2147483000;background:#111;color:#fff;border:none;border-radius:12px;padding:15px 28px;font:600 16px system-ui;cursor:pointer;box-shadow:0 8px 24px rgba(0,0,0,.3);display:none}#fl-add small{display:block;font-size:10px;font-weight:600;opacity:.7;letter-spacing:.05em;margin-top:2px}
`;

// The overlay's DOM, wrapped in a single #fl-root container so the
// MutationObserver can detect a wipe with one getElementById and re-mount the
// whole thing in one shot.
const ELEMENTS = `<div id="fl-ov"></div>
<button id="fl-add">Add to Cart<small>FORKLAUNCH COMMERCE</small></button>
<button id="fl-cart-btn">Cart <span class="n">0</span></button>
<div id="fl-drawer">
  <header><div><b>Your Cart</b><br><small>Powered by ForkLaunch</small></div><button onclick="FL.close()" style="background:none;border:none;font-size:22px;cursor:pointer">×</button></header>
  <div id="fl-items"></div>
  <div id="fl-foot"><div class="row"><span>Subtotal</span><span id="fl-sub">$0.00</span></div>
    <button onclick="FL.checkout()">Checkout</button><div class="fl-brand">Secured by ForkLaunch Commerce</div></div>
</div>
<div id="fl-co"><div class="fl-co-wrap">
  <div><span class="fl-badge">FORKLAUNCH CHECKOUT</span><h1>Checkout</h1>
    <div class="fl-field"><label>Email</label><input id="fl-email" placeholder="you@email.com" value="demo@forklaunch.com"></div>
    <div class="fl-field"><label>Shipping address</label><input placeholder="123 Main St" value="500 Market St"></div>
    <div class="fl-row2"><div class="fl-field"><label>City</label><input value="San Francisco"></div><div class="fl-field"><label>ZIP</label><input value="94105"></div></div>
    <div class="fl-field"><label>Card number</label><input placeholder="4242 4242 4242 4242" value="4242 4242 4242 4242"></div>
    <div class="fl-row2"><div class="fl-field"><label>Expiry</label><input value="12/28"></div><div class="fl-field"><label>CVC</label><input value="123"></div></div>
    <button id="fl-place" onclick="FL.place()">Place Order</button>
    <div class="fl-brand" style="margin-top:10px">Demo checkout — no real payment is processed</div>
  </div>
  <div class="fl-summary"><div id="fl-co-items"></div><div class="tot"><span>Total</span><span id="fl-co-tot">$0.00</span></div></div>
  <div id="fl-done"><div class="chk">✓</div><h1>Order confirmed</h1><p>Thanks! Your ForkLaunch order <b id="fl-ordno"></b> is placed.</p><button id="fl-place" style="max-width:200px" onclick="FL.reset()">Continue shopping</button></div>
</div></div>`;

const COMMERCE_HEAD = `<!--fl-commerce--><style>${CSS}</style>
<script>(function(){
  try{
  var KEY='_flc_demo';
  var ELEMENTS=${JSON.stringify(ELEMENTS)};
  function load(){try{return JSON.parse(localStorage.getItem(KEY))||{items:[]}}catch(e){return{items:[]}}}
  function save(c){try{localStorage.setItem(KEY,JSON.stringify(c))}catch(e){}}
  function money(c){return '$'+((c||0)/100).toFixed(2)}
  function esc(s){return String(s==null?'':s).replace(/[&<>"']/g,function(c){return {'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]})}
  function priceFromPage(){
    var m=(document.body&&document.body.innerText||'').match(/\\$\\s?([0-9]+(?:\\.[0-9]{2})?)/);
    return m?Math.round(parseFloat(m[1])*100):1500+Math.round(Math.random()*3000);
  }
  function titleFromPage(){var h=document.querySelector('h1');return (h&&h.innerText.trim())||document.title.split(/[|–\\-]/)[0].trim()||'Item';}
  function imgFromPage(){var i=document.querySelector('main img,.product img,[class*=product] img,img');return i?i.currentSrc||i.src:'';}
  function el(id){return document.getElementById(id)}
  function n(){return load().items.reduce(function(s,i){return s+i.quantity},0)}
  function sub(){return load().items.reduce(function(s,i){return s+i.price*i.quantity},0)}
  function render(){
    // Self-heal the count badge: some themes re-render the header in response
    // to the DOM change from opening the drawer and strip our .n span (seen on
    // hellotushy.com — add worked, cart populated, but the badge vanished so
    // the count read empty). Rebuild the button's guts if the span is gone.
    var btn=el('fl-cart-btn');
    if(btn&&!btn.querySelector('.n')){btn.innerHTML='Cart <span class="n">0</span>';}
    var cn=document.querySelector('#fl-cart-btn .n'); if(cn)cn.textContent=n();
    var c=load(),box=el('fl-items'); if(!box)return;
    if(!c.items.length){box.innerHTML='<div id="fl-empty">Your cart is empty.<br>Add something to get started.</div>';}
    else{box.innerHTML=c.items.map(function(i,ix){return '<div class="fl-it"><img src="'+esc(i.image||'')+'"><div class="t"><b>'+esc(i.title)+'</b><br>'+money(i.price)+' \\u00d7 '+i.quantity+'</div><button class="rm" onclick="FL.remove('+ix+')">Remove</button></div>'}).join('');}
    var s=el('fl-sub'); if(s)s.textContent=money(sub());
  }
  window.FL={
    add:function(o){var c=load();var f=c.items.find(function(i){return i.title===o.title});if(f)f.quantity++;else c.items.push({title:o.title,price:o.price,image:o.image,quantity:1});save(c);render();this.open();},
    remove:function(ix){var c=load();c.items.splice(ix,1);save(c);render();},
    open:function(){var d=el('fl-drawer'),o=el('fl-ov');if(d)d.classList.add('open');if(o)o.classList.add('open');},
    close:function(){var d=el('fl-drawer'),o=el('fl-ov');if(d)d.classList.remove('open');if(o)o.classList.remove('open');},
    checkout:function(){if(!n())return;var c=load();var ci=el('fl-co-items');if(ci)ci.innerHTML=c.items.map(function(i){return '<div class="li"><span>'+esc(i.title)+' \\u00d7 '+i.quantity+'</span><span>'+money(i.price*i.quantity)+'</span></div>'}).join('');var ct=el('fl-co-tot');if(ct)ct.textContent=money(sub());var co=el('fl-co');if(co)co.classList.add('open');},
    place:function(){var w=document.querySelector('#fl-co .fl-co-wrap');if(w)w.style.display='none';var d=el('fl-done');if(d)d.classList.add('open');var on=el('fl-ordno');if(on)on.textContent='#FL-'+Math.floor(100000+Math.random()*899999);localStorage.removeItem(KEY);},
    reset:function(){var co=el('fl-co');if(co)co.classList.remove('open');var w=document.querySelector('#fl-co .fl-co-wrap');if(w)w.style.display='';var d=el('fl-done');if(d)d.classList.remove('open');render();this.close();}
  };
  // Build (or rebuild) the overlay DOM into <body>. Idempotent: only acts when
  // #fl-root is missing, so it is safe to call repeatedly from the observer.
  function mount(){
    if(!document.body)return;
    if(!el('fl-root')){
      var root=document.createElement('div');
      root.id='fl-root';
      root.innerHTML=ELEMENTS;
      document.body.appendChild(root);
    }
    var cb=el('fl-cart-btn'); if(cb&&!cb._flw){cb._flw=1;cb.addEventListener('click',function(){FL.open()});}
    var ov=el('fl-ov'); if(ov&&!ov._flw){ov._flw=1;ov.addEventListener('click',function(){FL.close()});}
    if(/\\/products\\//.test(location.pathname)){
      var addb=el('fl-add');
      if(addb&&!addb._flw){addb._flw=1;var pr=priceFromPage();addb.innerHTML='Add to Cart &nbsp; '+money(pr)+'<small>FORKLAUNCH COMMERCE</small>';addb.style.display='block';addb.addEventListener('click',function(ev){ev.preventDefault();FL.add({title:titleFromPage(),price:pr,image:imgFromPage()});});}
    }
    render();
  }
  // Delegated add-to-cart intercept for the STORE'S OWN buy button (which may
  // be Sold Out / JS-broken offline). Skips our own overlay chrome entirely —
  // #fl-add is handled by its own listener above — so nothing double-adds.
  document.addEventListener('click',function(e){
    var t=e.target; if(!t||!t.closest)return;
    if(t.closest('#fl-root'))return;
    var b=t.closest('[name="add"],[data-action*="addCart"],[data-action*="add-to-cart"],.product-form__submit,button[class*="add"],[class*="add-to-cart"],[class*="AddToCart"],[class*="product-cta"],button,[type="submit"],a.button,a.btn,a[class*="button"]');
    if(!b)return;
    var txt=((b.innerText||b.value||b.getAttribute('aria-label')||'')+'').trim().toLowerCase();
    var known=b.matches('[name="add"],[data-action*="addCart" i],[data-action*="add-to-cart" i],.product-form__submit,[class*="add-to-cart"],[class*="product-cta"]');
    var isAdd = known || /add to cart|add to bag|add to basket|add to tote/.test(txt) || (/^add$/.test(txt) && !!b.closest('form[action*="/cart"],[class*="product"]'));
    if(isAdd){
      e.preventDefault();e.stopPropagation();
      FL.add({title:titleFromPage(),price:priceFromPage(),image:imgFromPage()});
    }
  },true);
  // Mount now if the body already exists, else at DOMContentLoaded.
  if(document.readyState==='loading'){document.addEventListener('DOMContentLoaded',mount);}else{mount();}
  // Self-heal: if a store's client hydration wipes body children — or a header
  // re-render strips just the count badge — put the overlay back. subtree so a
  // deep removal (the .n span) is caught, debounced so a busy page doesn't
  // thrash, and guarded so re-appending #fl-root doesn't loop.
  function watch(){try{var t=null;new MutationObserver(function(){
    if(t)return; t=setTimeout(function(){t=null;
      if(!el('fl-root')||!el('fl-cart-btn')||!document.querySelector('#fl-cart-btn .n'))mount();
    },50);
  }).observe(document.body,{childList:true,subtree:true});}catch(e){}}
  if(document.body)watch();else document.addEventListener('DOMContentLoaded',watch);
  }catch(err){try{console.warn('[fl-commerce] init failed',err)}catch(e){}}
})();</script>
`;

module.exports = { buildCommerceOverlay };
