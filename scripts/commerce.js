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
 * Reads/writes cart state in localStorage under `_fl_cart` — the SAME key
 * bridge.js's local-mode cart uses, so add-to-cart flows that go through a
 * theme's own JS (intercepted by bridge.js's fetch('/cart/add.js') patch)
 * and flows that go through this overlay's own click-delegation both land
 * in one cart.
 *
 * This is a byte-for-byte port of the overlay proven manually against
 * shopseeti.com and taylorstitch.com (add -> drawer -> checkout -> place
 * order -> "Order confirmed #FL-xxxxxx"), now wired into crawl.js's --clean
 * capture path so it is baked in at capture time instead of hand-injected.
 *
 * Exported as a function (mirrors bridge.js's buildBridge) so crawl.js can
 * inline it per page via injectBeforeBody — never via html.replace's
 * string-replacement form, which corrupts injected scripts containing
 * literal '$' sequences (see crawl.js's injectBeforeBody docstring).
 */
function buildCommerceOverlay() {
  return COMMERCE_HTML;
}

const COMMERCE_HTML = `<!--fl-commerce--><style>
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
</style>
<div id="fl-ov"></div>
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
</div></div>
<script>(function(){
  var KEY='_fl_cart';
  function load(){try{return JSON.parse(localStorage.getItem(KEY))||{items:[]}}catch(e){return{items:[]}}}
  function save(c){localStorage.setItem(KEY,JSON.stringify(c))}
  function money(c){return '$'+((c||0)/100).toFixed(2)}
  function priceFromPage(){
    var m=(document.body.innerText||'').match(/\\$\\s?([0-9]+(?:\\.[0-9]{2})?)/);
    return m?Math.round(parseFloat(m[1])*100):Math.round((1500+Math.random()*3000));
  }
  function titleFromPage(){var h=document.querySelector('h1');return (h&&h.innerText.trim())||document.title.split(/[|–\\-]/)[0].trim()||'Item';}
  function imgFromPage(){var i=document.querySelector('main img,.product img,[class*=product] img,img');return i?i.currentSrc||i.src:'';}
  function n(){return load().items.reduce(function(s,i){return s+i.quantity},0)}
  function sub(){return load().items.reduce(function(s,i){return s+i.price*i.quantity},0)}
  function render(){
    document.querySelector('#fl-cart-btn .n').textContent=n();
    var c=load(),box=document.getElementById('fl-items');
    if(!c.items.length){box.innerHTML='<div id="fl-empty">Your cart is empty.<br>Add something to get started.</div>';}
    else{box.innerHTML=c.items.map(function(i,ix){return '<div class="fl-it"><img src="'+(i.image||'')+'"><div class="t"><b>'+i.title+'</b><br>'+money(i.price)+' × '+i.quantity+'</div><button class="rm" onclick="FL.remove('+ix+')">Remove</button></div>'}).join('');}
    document.getElementById('fl-sub').textContent=money(sub());
  }
  window.FL={
    add:function(o){var c=load();var f=c.items.find(function(i){return i.title===o.title});if(f)f.quantity++;else c.items.push({title:o.title,price:o.price,image:o.image,quantity:1});save(c);render();this.open();},
    remove:function(ix){var c=load();c.items.splice(ix,1);save(c);render();},
    open:function(){document.getElementById('fl-drawer').classList.add('open');document.getElementById('fl-ov').classList.add('open');},
    close:function(){document.getElementById('fl-drawer').classList.remove('open');document.getElementById('fl-ov').classList.remove('open');},
    checkout:function(){if(!n())return;var c=load();document.getElementById('fl-co-items').innerHTML=c.items.map(function(i){return '<div class="li"><span>'+i.title+' × '+i.quantity+'</span><span>'+money(i.price*i.quantity)+'</span></div>'}).join('');document.getElementById('fl-co-tot').textContent=money(sub());document.getElementById('fl-co').classList.add('open');},
    place:function(){document.querySelector('#fl-co .fl-co-wrap').style.display='none';var d=document.getElementById('fl-done');d.classList.add('open');document.getElementById('fl-ordno').textContent='#FL-'+Math.floor(100000+Math.random()*899999);localStorage.removeItem(KEY);},
    reset:function(){document.getElementById('fl-co').classList.remove('open');document.querySelector('#fl-co .fl-co-wrap').style.display='';document.getElementById('fl-done').classList.remove('open');render();this.close();}
  };
  document.getElementById('fl-cart-btn').addEventListener('click',function(){FL.open()});
  // Guaranteed add-to-cart on product pages (store's own button may be Sold Out / JS-broken offline).
  if(/\\/products\\//.test(location.pathname)){
    var addb=document.getElementById('fl-add');
    var pr=priceFromPage();
    addb.innerHTML='Add to Cart &nbsp; '+money(pr)+'<small>FORKLAUNCH COMMERCE</small>';
    addb.style.display='block';
    addb.addEventListener('click',function(){FL.add({title:titleFromPage(),price:pr,image:imgFromPage()});});
  }
  document.getElementById('fl-ov').addEventListener('click',function(){FL.close()});
  // intercept ANY add-to-cart button on the page
  document.addEventListener('click',function(e){
    var t=e.target; if(!t||!t.closest)return;
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
  render();
})();</script>
`;

module.exports = { buildCommerceOverlay };
