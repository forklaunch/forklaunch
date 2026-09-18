#!/usr/bin/env bun
/**
 * heroserve-fl — serve a captured (--backend) storefront with its cart and
 * checkout wired to the REAL ForkLaunch ecommerce-stripe module over HTTP.
 *
 * This is the production wiring (as opposed to heroserve-pg.ts, which talks to
 * the local Postgres scaffold): a shopper browses the migrated visual clone,
 * and every add-to-cart / checkout call is proxied to the running ForkLaunch
 * module — the same module the catalog was imported into — authenticated with
 * the module's own HMAC scheme (@forklaunch/core's createHmacToken, reproduced
 * here so the skill stays self-contained).
 *
 *   bun heroserve-fl.ts <site-dir> <port> <module-url> <hmac-secret>
 *
 * The captured product page keeps its native Shopify add-to-cart form, whose
 * hidden `id` input is the source-platform (Shopify) variant id. The module
 * stored that as each variant's externalId, so we map handle + shopify-variant
 * -> the module's own variant UUID (via GET /product/handle/:handle) before
 * adding to the module cart. Payment (Stripe) is initiated by /checkout and
 * needs a real STRIPE_API_KEY in the module's env to complete the charge; the
 * order itself is created regardless.
 */
import { closeSync, existsSync, openSync, readdirSync, readFileSync, readSync, statSync } from 'node:fs';
import { brotliCompressSync, constants as zlibConstants, gzipSync } from 'node:zlib';
import { basename, dirname, extname, join } from 'node:path';
import { createHmac, randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { shopifyRuntime, getLocalCart } from './shopify-runtime.ts';
// The crawl names page files through urlmap.js (the single source of truth —
// blog posts fold to blogs/<blog>-<post>.html). A direct URL must resolve
// the same way, or a link that works when clicked 404s when pasted.
const { pageFileFor } = createRequire(import.meta.url)('../urlmap.js') as
  { pageFileFor: (p: string) => { file: string; depth: number } | null };

const [siteRoot, portArg, moduleUrl, secret] = process.argv.slice(2);
const PORT = Number(portArg ?? 4700);
const MODULE = (moduleUrl ?? 'http://localhost:8001').replace(/\/$/, '');
// MODULE has a default, so its value never tells you whether anyone actually
// asked for a backend. This does. /__fl/health reports it so a gate can say
// "skipped, not configured" instead of "failed".
const MODULE_CONFIGURED = !!moduleUrl;
const SECRET = secret ?? process.env.HMAC_SECRET_KEY ?? '';
// Stripe's publishable key (pk_...). Safe to serve to the browser — it can only
// create payment methods and confirm an intent whose client secret it was
// already handed; it cannot read the account or move money. Its presence is
// what turns checkout into a real card page: without it there is nothing to
// collect a card with, so checkout falls back to creating the order alone
// (the pre-existing behaviour, still what a keyless visual demo wants).
const STRIPE_PK = process.argv[6] ?? process.env.STRIPE_PUBLISHABLE_KEY ?? '';
// PayPal's client id is likewise public — the JS SDK is loaded with it in the
// script URL on every PayPal-enabled storefront. Present means the checkout
// offers a PayPal button alongside the card form.
const PAYPAL_CLIENT_ID = process.argv[7] ?? process.env.PAYPAL_CLIENT_ID ?? '';

// --- HMAC exactly as @forklaunch/core's createHmacToken builds it -----------
// message = `${method}\n${path}\n${bodyString}${timestamp}\n${nonce}` where
// bodyString = body ? JSON.stringify(body)+'\n' : the literal string 'undefined'
// (a no-body request signs `${undefined}`, which stringifies to "undefined").
// `path` is the handler path RELATIVE to its router mount, not the request URL.
function authHeader(method: string, signedPath: string, body?: unknown): string {
  const ts = new Date();
  const nonce = randomUUID();
  const bodyString = body != null ? `${JSON.stringify(body)}\n` : 'undefined';
  const sig = createHmac('sha256', SECRET)
    .update(`${method}\n${signedPath}\n${bodyString}${ts.toISOString()}\n${nonce}`)
    .digest('base64');
  return `HMAC keyId=default ts=${ts.toISOString()} nonce=${nonce} signature=${sig}`;
}

async function mod(method: string, url: string, signedPath: string, body?: unknown) {
  const res = await fetch(MODULE + url, {
    method,
    headers: { 'content-type': 'application/json', authorization: authHeader(method, signedPath, body) },
    body: body != null ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  let json: any; try { json = JSON.parse(text); } catch { json = text; }
  return { code: res.status, body: json };
}

// Prefilled so a demo shopper can click through without inventing an address;
// the checkout page still lets them edit every field. The module requires a
// complete address to price shipping and tax.
const DEMO_ADDRESS = { name: 'ForkLaunch Demo', line1: '500 Market St', city: 'San Francisco', state: 'CA', postalCode: '94105', country: 'US' };

// One demo cart per server run (mirrors heroserve-pg's single-cart model).
let cartId: string | null = null;
async function ensureCart(): Promise<string> {
  if (cartId) return cartId;
  const r = await mod('POST', '/cart', '/', { customerId: 'fl-demo-customer' });
  cartId = r.body?.id;
  return cartId!;
}

// Map a source-platform variant id (from the captured page) to the module's
// own variant UUID via the product's handle.
// Catalog data does not change while the server is up, so every lookup below
// is cached. Without this each add-to-cart re-resolved the same handle and
// re-fetched the same variant, and the cart redraw did it again per line —
// six HMAC-signed round trips for one click, all sequential.
const variantIdCache = new Map<string, string | null>();
const variantCache = new Map<string, { priceCents: number; title: string; productId?: string }>();

async function resolveVariant(handle: string, shopifyVariantId?: string): Promise<string | null> {
  const key = handle + '\u0000' + (shopifyVariantId ?? '');
  const hit = variantIdCache.get(key);
  if (hit !== undefined) return hit;
  const resolved = await resolveVariantUncached(handle, shopifyVariantId);
  // Only cache a hit. A miss is usually transient — the module restarting, the
  // database briefly unreachable — and caching it pins a 404 on that product
  // for the life of the process even after the module comes back.
  if (resolved !== null) variantIdCache.set(key, resolved);
  return resolved;
}

async function resolveVariantUncached(handle: string, shopifyVariantId?: string): Promise<string | null> {
  const p = await mod('GET', `/product/handle/${handle}`, `/handle/${handle}`);
  if (p.code !== 200) return null;
  const productId = p.body.id;
  const vs = await mod('GET', `/variant/product/${productId}`, `/product/${productId}`);
  if (vs.code !== 200 || !Array.isArray(vs.body) || !vs.body.length) return null;
  if (shopifyVariantId) {
    const hit = vs.body.find((v: any) => String(v.externalId) === String(shopifyVariantId));
    if (hit) return hit.id;
  }
  return vs.body[0].id; // fall back to the first variant
}

/**
 * Per-capture control map, written by discover-controls.mjs.
 *
 * The add-to-cart interception below works on every theme ever shipped because
 * `POST /cart/add.js` is a CONTRACT — intercept the path and you have
 * intercepted the feature, whatever the button looks like. Checkout has no
 * such contract, and it used to be matched with `[name="checkout"],
 * [href*="/checkout"], [href="/cart"]`: an accurate description of Dawn and
 * its descendants, and of nothing else.
 *
 * When that guess misses, the failure is silent and it is the worst one
 * available — the button is present, it looks right, and clicking it walks the
 * viewer out of the demo and onto the merchant's real Shopify checkout.
 *
 * So checkout is now identified three ways, weakest last: the /cart/add
 * contract (for what to LEAVE ALONE), then the control's accessible name
 * computed live in the page, then selectors this file discovered for this
 * particular capture. Missing file is fine — the name matcher alone still
 * covers every theme that labels its button in words.
 */
let CONTROLS: { kinds?: Record<string, { sel: string }[]>; names?: Record<string, string[]> } = {};
try {
  const cf = join(siteRoot, '_fl-controls.json');
  if (existsSync(cf)) CONTROLS = JSON.parse(readFileSync(cf, 'utf8'));
} catch (_) { /* a corrupt map is no worse than no map */ }

const MIME: Record<string, string> = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css',
  '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.avif': 'image/avif', '.gif': 'image/gif',
  '.woff': 'font/woff', '.woff2': 'font/woff2', '.ico': 'image/x-icon',
  // Video types matter more than they look. Served as the
  // application/octet-stream fallback, a <video> source is not media the
  // browser can stream — it fetches the whole file up front instead of
  // range-requesting as it plays, and preload="none" cannot help because the
  // response never looks like media. On graza.co that was 34.5MB across nine
  // files for one homepage visit.
  '.mp4': 'video/mp4', '.m4v': 'video/mp4', '.webm': 'video/webm',
  '.mov': 'video/quicktime', '.ogv': 'video/ogg',
  '.m3u8': 'application/vnd.apple.mpegurl', '.ts': 'video/mp2t',
  '.mp3': 'audio/mpeg', '.m4a': 'audio/mp4', '.ogg': 'audio/ogg',
};

// Client shim injected into every served HTML page: intercept the native
// Shopify cart calls and the checkout click, and route them to /__fl/*.
// Squarespace fades content in from JS (`preFade`/`preSlide`/`preScale`); with
// that runtime blocked offline, anything captured mid-animation stays hidden,
// and the add-to-cart wrapper can be captured as display:none. Force them
// shown. A merchant that switched selling off hides the button in its own CSS
// with !important (enquiry-only stores do); the html body prefix outranks that.
// The rules are no-ops on any other platform.
const SQS_STYLE = `<style>.preFade,.preSlide,.preScale,[data-animation-role]{opacity:1!important;visibility:visible!important;transform:none!important}html body .sqs-add-to-cart-button-wrapper,html body .sqs-add-to-cart-button{display:flex!important;opacity:1!important;visibility:visible!important}html body .sqs-add-to-cart-button{align-items:center;justify-content:center}.cookie-banner-mount-point,.gdpr-cookie-banner{display:none!important}</style>`;
const SHIM = `<script>(function(){
  // Presentation, for a store being shown to a client. commerce.js is built
  // for a standalone offline demo, so it labels its own controls — a
  // "FORKLAUNCH COMMERCE" caption on the buy button and a "Secured by
  // ForkLaunch Commerce" line in the drawer. On a migration of somebody's
  // real storefront that reads as a watermark on their brand.
  //
  // The theme's own header counter is the other half: it renders from the
  // platform cart this bridge replaced, so it sits at zero no matter what the
  // shopper adds, and the overlay's separate button means two disagreeing
  // counts on screen. Keeping the theme's counter truthful is what makes the
  // page feel native, so it is updated from the module's cart instead.
  function flDebrand(){
    document.querySelectorAll('#fl-add small').forEach(function(n){n.remove()});
    document.querySelectorAll('.fl-brand, #fl-drawer header small').forEach(function(n){
      if(/forklaunch/i.test(n.textContent||''))n.remove();
    });
  }

  // The theme writes its count as "Cart [0]", "Cart (0)", "Cart 0" or a bare
  // number in a dedicated node, depending on the store. Rewriting the digits
  // inside whatever node already displays it keeps the theme's own styling.
  function flSyncCount(n){
    try{
      // The count is rarely a single text node — themes wrap it, so
      // "Cart[0]" is often three nodes deep. Match on the element's whole
      // text, then rewrite the digits in whichever descendant text node
      // actually holds them, which preserves the theme's own markup.
      var els=document.querySelectorAll('a,button,span,div');
      for(var i=0;i<els.length;i++){
        var e=els[i];
        var t=(e.textContent||'').trim();
        if(t.length>12)continue;
        if(!/^cart\\s*[\\[\\(]?\\s*\\d+\\s*[\\]\\)]?$/i.test(t))continue;
        var w=document.createTreeWalker(e,NodeFilter.SHOW_TEXT,null);
        var node;
        while((node=w.nextNode())){
          if(/\\d/.test(node.nodeValue||'')){
            node.nodeValue=node.nodeValue.replace(/\\d+/,String(n));
            break;
          }
        }
      }
    }catch(e){}
  }

  async function flRefreshCount(){
    try{
      var r=await of('/__fl/cart');
      var c=await r.json();
      flSyncCount(c && typeof c.item_count==='number' ? c.item_count : 0);
      // The overlay drawer renders from localStorage, which outlives the
      // module's cart (a server restart, a paid order). Mirror the module's
      // cart into it so the drawer never shows a line checkout will not charge.
      try{localStorage.setItem('_flc_demo',JSON.stringify({items:((c&&c.items)||[]).map(function(i){return{title:i.product_title||i.title,price:i.price,image:i.image||'',quantity:i.quantity}})}))}catch(e){}
    }catch(e){}
  }
  document.addEventListener('DOMContentLoaded',function(){
    flDebrand(); flRefreshCount();
    new MutationObserver(flDebrand).observe(document.documentElement,{childList:true,subtree:true});
  });

  // Diagnostic marker. Whether this script executes at all depends on where
  // the parser relocates it and whether a captured inline script has flipped
  // the tokenizer into raw-text mode; without a marker that is invisible and
  // easy to mistake for a logic bug.
  window.__flShim = (window.__flShim || 0) + 1;

  // Dead overlays. A third-party pop-up captured in its OPEN state (Alia's
  // scratch card on gorillamind.com sat over the header and product hero on
  // all 44 pages) can never be closed here: its vendor script is blocked.
  // Removed on evidence, not on a guess — a known pop-up vendor's root, or a
  // fixed element at the browser's maximum z-index covering most of the
  // viewport, a position only injected pop-ups occupy. Marked with
  // data-fl-dead-overlay so the report can say what was removed and why.
  var FL_POPUP_SEL='[id^="alia-popup-root"],.klaviyo-form-overlay,[class*="kl-private-reset-css"] [data-testid="POPUP"],.needsclick[data-testid="POPUP"],.privy-popup-container,#privy-popup-container,[id^="justuno"],[id^="ju_Con"],.wisepops-root,[id^="wisepops"],[id^="attentive_overlay"],[id^="attentive_creative"],.ps-popup,[id^="ps-popup"],.optimonk-overlay,[id^="wheelio"],[id^="tada-"],.sleeknote-overlay,[id^="sleeknote"]';
  var flKillPending=0;
  function flKillOverlays(){
    flKillPending=0;
    try{
      var i,n,els=document.querySelectorAll(FL_POPUP_SEL);
      for(i=0;i<els.length;i++){n=els[i];if(n.__flDead)continue;n.__flDead=1;n.setAttribute('data-fl-dead-overlay','vendor');n.style.setProperty('display','none','important');}
      if(!document.body)return;
      var vw=window.innerWidth,vh=window.innerHeight,all=document.body.children;
      for(i=0;i<all.length;i++){n=all[i];if(n.__flDead||/^(SCRIPT|STYLE|LINK|HEADER|NAV|MAIN|FOOTER|TEMPLATE)$/.test(n.tagName))continue;
        if(n.id&&/^fl-|^__fl/.test(n.id))continue;
        var cs=getComputedStyle(n);if(cs.position!=='fixed'&&cs.position!=='absolute')continue;
        var z=parseInt(cs.zIndex,10);if(!(z>=2147483000))continue;
        if(cs.display==='none'||cs.visibility==='hidden')continue;
        var r=n.getBoundingClientRect();if(r.width*r.height<0.5*vw*vh)continue;
        n.__flDead=1;n.setAttribute('data-fl-dead-overlay','max-z');n.style.setProperty('display','none','important');}
      if(document.querySelector('[data-fl-dead-overlay]')){
        var bo=getComputedStyle(document.body).overflow;
        if(/hidden/.test(bo))document.body.style.setProperty('overflow','auto','important');
        var ho=getComputedStyle(document.documentElement).overflow;
        if(/hidden/.test(ho))document.documentElement.style.setProperty('overflow','auto','important');
      }
    }catch(e){}
  }
  function flKillSoon(){if(flKillPending)return;flKillPending=1;setTimeout(flKillOverlays,150);}
  document.addEventListener('DOMContentLoaded',flKillOverlays);
  setTimeout(flKillOverlays,300);setTimeout(flKillOverlays,1500);setTimeout(flKillOverlays,4000);
  try{new MutationObserver(flKillSoon).observe(document.documentElement,{childList:true,subtree:true});}catch(e){}

  // Serve-time rewriting stripped autoplay and set preload=none so nothing
  // downloads up front. Restore the original look by loading and playing each
  // video only while it is on screen, pausing it when it leaves — so at most
  // the one or two in view are ever decoding, instead of all five at once.
  // Pausing reactively cannot win: the theme re-calls play() on its own
  // videos after we pause them, so three stayed running while invisible.
  // Gate play() itself instead — an off-screen video simply does not start,
  // and the IntersectionObserver above starts it when it scrolls into view.
  try{
    var _play = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = function(){
      try{
        if(this.tagName === 'VIDEO' && !this.__flWantsPlay){
          var b = this.getBoundingClientRect();
          var cs = getComputedStyle(this);
          var onScreen = b.width > 10 && b.height > 10 &&
            cs.visibility !== 'hidden' && cs.display !== 'none' &&
            b.bottom > -200 && b.top < (innerHeight + 200);
          if(!onScreen) return Promise.resolve();
        }
      }catch(e){}
      return _play.apply(this, arguments);
    };
  }catch(e){}

  function flLazyVideo(v){
    if(v.__flLazy)return; v.__flLazy=1;
    // Videos the client renders (React/Hydrogen builds some of its media after
    // hydration) never passed through the serve-time rewrite, so they still
    // carry the theme's autoplay and start downloading on creation. Strip it
    // here too, and stop anything already running: on graza.co's homepage
    // three autoplaying videos were decoding while invisible.
    try{ v.removeAttribute('autoplay'); v.autoplay=false; v.muted=true;
         if(!v.paused) v.pause();
         if(v.preload!=='none' && v.readyState===0) v.preload='none'; }catch(e){}
    // Only ONE video is ever allowed to decode at a time.
    //
    // Hiding invisible video was not enough. A storefront homepage stacks
    // video sections, and with a 200px root margin three or four of them are
    // legitimately on screen at once during a scroll. Each one is a separate
    // hardware decode session, and the media engine has only a few; past that
    // the system falls back to decoding on the CPU. On an 8GB laptop that
    // starved the compositor badly enough that macOS's watchdog declared
    // WindowServer unresponsive after 40 seconds and killed it, which logs
    // the user out and reads to them as the machine crashing.
    //
    // The arbiter keeps the most-visible video playing and pauses the rest.
    // A storefront never intends several videos to compete for attention
    // anyway, so nothing is lost visually: whichever one the shopper is
    // actually looking at is the one that runs.
    //
    // preload is also granted only to the winner. Setting it to 'auto' on
    // every on-screen video pulled all of their bytes at once, which is the
    // same problem one layer down.
    // Defined whenever it is missing, not behind a once-per-window flag: the
    // arbiter is stateless (it re-reads the DOM on every call), so redefining
    // it is harmless, whereas a flag set without the function — seen as
    // "flArbitrate is not defined" on gorillamind.com — leaves every observer
    // below calling a name that does not exist.
    if(typeof window.flArbitrate!=='function'){
      window.flArbitrate=function(){
        var vs=document.querySelectorAll('video'),best=null,bestR=-1;
        for(var i=0;i<vs.length;i++){
          var v=vs[i];
          if(!v.__flWantsPlay)continue;
          var r=v.__flRatio||0;
          if(r>bestR){bestR=r;best=v;}
        }
        for(var j=0;j<vs.length;j++){
          var w=vs[j];
          if(w===best){
            if(w.preload!=='auto')w.preload='auto';
            if(w.paused){var pr=w.play();if(pr&&pr.catch)pr.catch(function(){});}
          }else if(!w.paused){
            try{w.pause()}catch(e){}
          }
        }
      };
    }
    if(!('IntersectionObserver' in window)){v.preload='auto';return;}
    new IntersectionObserver(function(es){
      es.forEach(function(e){
        // isIntersecting is geometry only. A video can sit inside the
        // viewport while visibility:hidden or display:none — a carousel
        // slide, a closed drawer — and IntersectionObserver still reports it
        // as intersecting, so it would play and keep playing unseen. Check
        // that it is genuinely rendered too.
        var cs=getComputedStyle(v);
        var shown=e.isIntersecting && cs.visibility!=='hidden' && cs.display!=='none' && v.offsetParent!==null;
        if(shown){
          // Restore the deferred source the first time it is genuinely shown.
          if(!v.__flSrcOn){
            v.__flSrcOn=1;
            var ds=v.getAttribute('data-fl-src'); if(ds)v.src=ds;
            var ss=v.querySelectorAll('source[data-fl-src]');
            for(var k=0;k<ss.length;k++)ss[k].src=ss[k].getAttribute('data-fl-src');
            if(ds||ss.length)try{v.load()}catch(err){}
          }
          v.muted=true;v.__flWantsPlay=1;v.__flRatio=e.intersectionRatio||0;
          if(window.flArbitrate)window.flArbitrate();
        }
        else{v.__flWantsPlay=0;v.__flRatio=0;try{v.pause()}catch(err){}if(window.flArbitrate)window.flArbitrate();}
      });
    },{rootMargin:'200px'}).observe(v);
  }
  // A video can also be hidden *after* it starts — hydration swapping a
  // carousel slide, a drawer closing. The play() gate only judges at call
  // time, so sweep periodically and stop anything now playing unseen.
  setInterval(function(){
    var vs=document.querySelectorAll('video');
    for(var i=0;i<vs.length;i++){
      var v=vs[i];
      if(v.paused)continue;
      var cs=getComputedStyle(v);
      if(cs.visibility==='hidden'||cs.display==='none'||v.offsetParent===null){
        v.__flWantsPlay=0; v.__flRatio=0; try{v.pause()}catch(e){}
      }
    }
    if(window.flArbitrate)window.flArbitrate();
  },1000);

  function flScanVideos(root){
    if(!root||!root.querySelectorAll)return;
    var vs=root.querySelectorAll('video');
    for(var i=0;i<vs.length;i++)flLazyVideo(vs[i]);
  }
  document.addEventListener('DOMContentLoaded',function(){flScanVideos(document)});
  try{
    new MutationObserver(function(ms){
      for(var i=0;i<ms.length;i++){
        var an=ms[i].addedNodes;
        for(var j=0;j<an.length;j++){
          var n=an[j];
          if(n.nodeType===1){
            if(n.matches&&n.matches('video'))flLazyVideo(n);
            flScanVideos(n);
          }
        }
      }
    }).observe(document.documentElement,{childList:true,subtree:true});
  }catch(e){}

  // Capture-time development furniture. Both are baked into the saved pages,
  // and both look like defects to anyone being shown the migrated store: the
  // #_mirror-index panel is a crawl debug list, and the overlay's floating
  // Cart button sits on top of the theme's own cart control. Hidden with CSS
  // rather than removed, because the overlay's own MutationObserver rebuilds
  // its DOM whenever it is torn out.
  (function(){
    var css='#_mirror-index{display:none!important}';
    var st=document.createElement('style');st.id='fl-serve-css';
    st.textContent=css;
    (document.head||document.documentElement).appendChild(st);
    // The overlay's cart button only earns its place when the theme has no
    // working cart control of its own — that is the case commerce.js was
    // written for. When the theme does have one, ours is a duplicate.
    // The overlay's cart button is moved out of the theme's way rather than
    // hidden. Hiding it looks tidier but is wrong: the theme's own counter
    // reads from the source platform's cart, which this migration replaced, so
    // it sits at 0 no matter what the shopper adds (graza.co renders
    // Cart[0] no matter what). The overlay's button is the only cart
    // the module actually holds, so it has to stay visible — just not on top
    // of the theme's.
    var pos=document.createElement('style');pos.id='fl-cart-pos';
    pos.textContent='#fl-cart-btn{top:auto!important;bottom:20px!important;right:20px!important}';
    (document.head||document.documentElement).appendChild(pos);
  })();
  // Shopify /products/<h>; Squarespace /<collection>/p/<h>. Same handle the catalog was imported under.
  function handle(){var m=location.pathname.match(/\\/products\\/([^/?#]+?)(?:\\.html)?$/)||location.pathname.match(/\\/[^/]+\\/p\\/([^/?#]+?)(?:\\.html)?$/)||location.pathname.match(/\\/[^/]+\\/p-([^/?#]+?)\\.html$/);return m?m[1]:'';}
  var of=window.fetch;
  window.fetch=function(u,o){
    try{
      var url=(''+((u&&u.url)||u));
      if(/\\/cart\\/add(\\.js)?/.test(url)){
        var id='';try{var b=o&&o.body;if(b instanceof FormData){id=b.get('id')}else if(typeof b==='string'){var mm=b.match(/[?&]?id=([^&]+)/);id=mm?decodeURIComponent(mm[1]):(JSON.parse(b).id||'')}}catch(e){}
        return of('/__fl/add',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({handle:handle(),variantExternalId:String(id||'')})}).then(function(r){return r.json()}).then(function(c){flToast();flSyncCount(c&&c.item_count||0);return new Response(JSON.stringify(c),{headers:{'content-type':'application/json'}})});
      }
      if(/\\/cart(\\.js|\\.json)(\\?|$)/.test(url))return of('/__fl/cart');
    }catch(e){}
    return of(u,o);
  };
  function flToast(){var t=document.getElementById('fl-toast');if(!t){t=document.createElement('div');t.id='fl-toast';t.style.cssText='position:fixed;top:16px;right:16px;z-index:2147483000;background:#111;color:#fff;padding:12px 18px;border-radius:10px;font:600 14px system-ui;box-shadow:0 6px 20px rgba(0,0,0,.3)';document.body.appendChild(t)}t.innerHTML='\\u2713 Added to cart &nbsp; <a href="/__fl/checkout" style="color:#8bf">Checkout \\u2192</a>';}
  document.addEventListener('submit',function(e){var f=e.target;if(f&&f.action&&/\\/cart\\/add/.test(f.action)){e.preventDefault();var fd=new FormData(f);window.fetch('/cart/add.js',{method:'POST',body:fd});}},true);

  // ---- Squarespace add-to-cart -------------------------------------------
  // A captured Squarespace product page keeps its own button but the script
  // behind it (Squarespace's commerce runtime) is blocked offline. The page's
  // handle is the last path segment (/shop/p/<handle>), which is also the
  // handle the catalog was imported under, so one call resolves the variant.
  // No regex literals here, same reason as the block below.
  document.addEventListener('click',function(e){
    var el=e.target;while(el&&el!==document&&!(el.classList&&el.classList.contains('sqs-add-to-cart-button')))el=el.parentNode;
    if(!el||el===document)return;
    // With the demo overlay present its bound FL.add already drives this
    // button; a second add here would double every line. Only step in when
    // there is no overlay (a capture made without --clean).
    if(window.FL&&window.FL.__flBound)return;
    e.preventDefault();e.stopPropagation();
    var parts=location.pathname.split('/').filter(function(s){return s});var h=parts[parts.length-1]||'';
    if(h.slice(-5)==='.html')h=h.slice(0,-5);
    if(h.slice(0,2)==='p-')h=h.slice(2); // the capture flattens /shop/p/<h> to /shop/p-<h>.html
    var inner=el.querySelector('.sqs-add-to-cart-button-inner')||el;var was=inner.textContent;inner.textContent='Adding...';
    window.fetch('/__fl/add',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({handle:h,variantExternalId:''})})
      .then(function(r){return r.json()}).then(function(c){if(c&&c.error)throw new Error(c.error);flToast();flSyncCount(c&&c.item_count||0);inner.textContent='Added - go to checkout';el.onclick=function(ev){ev.preventDefault();location.href='/__fl/checkout'};})
      .catch(function(){inner.textContent='Unavailable';setTimeout(function(){inner.textContent=was},1600)});
  },true);

  // ---- checkout interception, discovered rather than assumed ----------------
  // Three layers, weakest last. Layer 2 (the accessible name, computed here at
  // click time) is the one that makes this theme-independent: it also catches
  // controls that did not exist when the capture was walked, such as the
  // Checkout button inside a cart drawer that is built when the drawer opens.
  //
  // No regex literals and no string escapes in this block on purpose. This
  // source passes through a template literal and then into HTML before a
  // browser parses it, and a backslash that survives one layer does not always
  // survive the next — a dropped escape here silently kills every script on
  // the page, which has happened three times.
  var FLC = ${JSON.stringify(CONTROLS)};
  function flNorm(s){s=String(s||'');var o='',p=true;for(var i=0;i<s.length;i++){var c=s.charCodeAt(i);if(c<=32){if(!p){o+=' ';p=true}}else{o+=s[i];p=false}}return o.trim().toLowerCase().slice(0,120)}
  function flName(el){
    var s=(el.getAttribute&&el.getAttribute('aria-label'))||'';
    if(!flNorm(s))s=el.textContent||'';
    if(!flNorm(s)){var im=el.querySelector&&el.querySelector('img[alt]');if(im)s=im.getAttribute('alt')||''}
    if(!flNorm(s))s=(el.getAttribute&&(el.getAttribute('title')||el.getAttribute('name')))||'';
    return flNorm(s);
  }
  var FLRE={
    checkout:new RegExp('^(checkout|check out|proceed to checkout|continue to checkout|place (the )?order|pay now|complete (my )?order|secure checkout|go to checkout|buy now|buy it now)'),
    cart:new RegExp('^(cart|bag|basket|shopping (cart|bag|basket)|view (cart|bag)|open (cart|bag)|your (cart|bag)|my (cart|bag))'),
    // "Buy it now" is NOT in this list. Shopify's dynamic checkout button does
    // not add to a cart — it jumps straight to the hosted checkout, so
    // exempting it from interception walks the viewer out of the demo and onto
    // the merchant's real store mid-presentation. It is classified as checkout
    // above, where it belongs.
    add:new RegExp('^(add to (cart|bag|basket|order)|add$|add to my (cart|bag))')
  };
  function flMatchesSel(el,kind){
    var list=(FLC.kinds&&FLC.kinds[kind])||[];
    for(var i=0;i<list.length;i++){try{if(el.matches&&el.matches(list[i].sel))return true}catch(_e){}}
    return false;
  }
  function flIsKind(el,kind){
    if(flMatchesSel(el,kind))return true;
    var n=flName(el);
    if(!n)return false;
    var known=(FLC.names&&FLC.names[kind])||[];
    if(known.indexOf(n)!==-1)return true;
    return FLRE[kind]?FLRE[kind].test(n):false;
  }
  document.addEventListener('click',function(e){
    var el=e.target.closest&&e.target.closest('a,button,input,summary,[role=button],[onclick]');
    if(!el)return;
    // Add-to-cart is intercepted by the /cart/add contract above, not here.
    // Catching it twice would send a shopper to checkout instead of adding
    // their item, so the specific case is checked first and bails out.
    var form=el.closest&&el.closest('form');
    var action=(form&&form.getAttribute('action'))||'';
    if(action.indexOf('/cart/add')!==-1||FLRE.add.test(flName(el)))return;
    var href=(el.getAttribute&&el.getAttribute('href'))||'';
    var isCheckout=flIsKind(el,'checkout')||href.indexOf('/checkout')!==-1;
    var isCart=flIsKind(el,'cart')||href==='/cart'||href.indexOf('/cart?')===0;
    if(isCheckout||isCart){e.preventDefault();e.stopPropagation();location.href='/__fl/checkout';}
  },true);

  // The capture's asset localiser treats the site root as a fetchable asset,
  // so a logo whose href was "/" comes back rewritten to something like
  // _a/other/index.<hash>.bin. Clicking it downloads that file instead of
  // going home. A .bin file is the capture's unknown-type bucket and is never a
  // legitimate destination for a link a shopper can click, so route any of
  // them back to the root.
  document.addEventListener('click',function(e){
    var a=e.target.closest&&e.target.closest('a[href]');
    if(!a)return;
    var h=a.getAttribute('href')||'';
    var q=h.split('?')[0].split('#')[0];
    // Plain string tests rather than a regex on purpose: this source passes
    // through a template literal and then into HTML before a browser parses
    // it, and a backslash that survives one layer does not always survive the
    // next. A dropped escape here silently kills every script on the page.
    if(q.indexOf('_a/')!==-1&&q.slice(-4)==='.bin'){e.preventDefault();location.href='/';}
  },true);

  // commerce.js — the offline demo overlay crawl.js inlines into <head> at
  // capture time — owns the visible Cart and Checkout buttons on every
  // captured page, and it is entirely self-contained: FL.add writes straight
  // to localStorage (it makes no fetch call at all, so the interception above
  // cannot see it) and FL.checkout renders a fake card form that charges
  // nothing. That is right for a keyless visual demo and wrong here, where the
  // whole point is that the storefront drives the real module. Repoint its
  // three entry points at the bridge. A property trap rather than a direct
  // patch because this runs before commerce.js assigns window.FL.
  function bind(FL){
    if(!FL||FL.__flBound)return FL;
    var origAdd=FL.add;
    // The overlay's own add scrapes the price off the page, which is wrong on
    // any theme that hides it (an enquiry-only store) and drifts from the
    // module on every other. Ask the module first and mirror ITS cart into
    // the overlay's store, so the drawer shows the same money checkout will.
    // FL.remove with an out-of-range index is the overlay's only exposed way
    // to re-render from that store without changing it.
    FL.add=function(o){
      of('/__fl/add',{method:'POST',headers:{'content-type':'application/json'},
        body:JSON.stringify({handle:handle(),variantExternalId:''})})
        .then(function(r){return r.json()})
        .then(function(c){
          if(!c||c.error)throw new Error(c&&c.error||'add failed');
          try{localStorage.setItem('_flc_demo',JSON.stringify({items:(c.items||[]).map(function(i){return{title:i.product_title||i.title,price:i.price,image:(o&&o.image)||i.image||'',quantity:i.quantity}})}))}catch(e){}
          FL.remove(1e9);FL.open();flDebrand();flToast();flRefreshCount();
        })
        .catch(function(e){console.warn('[fl] module add failed, drawer falling back to page price:',e&&e.message||e);try{origAdd.call(FL,o)}catch(e2){}});
    };
    FL.checkout=function(){location.href='/__fl/checkout'};
    FL.place=function(){location.href='/__fl/checkout'};
    FL.__flBound=true;
    return FL;
  }
  if(window.FL)bind(window.FL);
  else{try{var _fl;Object.defineProperty(window,'FL',{configurable:true,
    get:function(){return _fl},set:function(v){_fl=bind(v)}});}catch(e){}}
})();</script>`;

/**
 * Captured storefronts are far heavier than they look. One graza.co product
 * page pulls 632 requests and 63MB, of which 49MB is five hero videos the
 * theme marks autoplay+loop — so each downloads in full and runs a decoder for
 * as long as the tab is open, regardless of the preload="metadata" hint that
 * autoplay overrides. On a low-memory machine that alone is fatal. It is the
 * merchant's own page weight faithfully reproduced, not something the
 * migration added, but a demo has no reason to pay it.
 *
 * Rewritten at serve time rather than at runtime because by the time an
 * injected script runs the parser has already begun fetching. The look is
 * preserved: the runtime half of this plays each video while it is on screen,
 * so it still autoplays and loops as the shopper scrolls to it.
 */
/**
 * Absolutise capture paths that appear INSIDE <script> blocks.
 *
 * The crawl rewrites asset references to paths relative to the site root:
 * `_a/js/foo.<hash>.js`, no leading slash. In an HTML attribute that is fine —
 * the browser resolves it against the document. Inside a script it is not,
 * because a module specifier is not a URL: `import("_a/js/foo.js")` is a BARE
 * specifier, and the browser rejects it outright with "Failed to resolve
 * module specifier" rather than trying to fetch anything.
 *
 * graza.co's homepage carries Shopify's shop-js loader, whose module table the
 * crawl faithfully rewrote into exactly that shape. The result: an uncaught
 * error on every page load and one of the theme's modules never running. The
 * page looked completely fine, which is the recurring theme of every defect
 * this file works around.
 *
 * Done at serve time and scoped to script content only, because the same
 * relative form in an `href` or `src` attribute IS correct and rewriting it
 * would break the pages that rely on it.
 */
function absolutiseScriptPaths(html: string): string {
  return html.replace(/<script\b[^>]*>([\s\S]*?)<\/script\s*>/gi, (whole, inner) => {
    if (!inner.includes('_a/')) return whole;
    const fixed = inner.replace(/(["'`])_a\//g, '$1/_a/');
    return fixed === inner ? whole : whole.replace(inner, fixed);
  });
}

function lightenMedia(html: string): string {
  return html
    // Deferring the source, not just the playback. preload="none" is only a
    // hint — the theme's own script calls load() or play() and the browser
    // fetches the whole file regardless, which is how a 13MB hero video still
    // arrived on a page where it was never visible. An element with no src
    // has nothing to fetch; the runtime half restores it when the video is
    // actually shown.
    .replace(/<video\b[^>]*>/gi, (tag) =>
      tag
        .replace(/\sautoplay(=(["'])[^"']*\2)?/gi, '')
        .replace(/\spreload=(["'])[^"']*\1/gi, '')
        .replace(/\ssrc=/gi, ' data-fl-src=')
        .replace(/^<video\b/i, '<video data-fl-lazy muted playsinline preload="none"'))
    .replace(/<source\b[^>]*>/gi, (tag) =>
      /type=(["'])video/i.test(tag) || /\.(mp4|webm|mov|m4v|bin)(\?|["'])/i.test(tag)
        ? tag.replace(/\ssrc=/gi, ' data-fl-src=')
        : tag)
    .replace(/<link\b[^>]*\brel=(["'])prefetch\1[^>]*>/gi, '')
    // Below-the-fold images cost another 1.4MB up front on that same page.
    .replace(/<img\b(?![^>]*\sloading=)[^>]*>/gi, (tag) =>
      tag.replace(/^<img\b/i, '<img loading="lazy" decoding="async"'));
}

/**
 * The capture writes anything it cannot classify into _a/other with a .bin
 * extension, and on a Shopify storefront that bucket is mostly video: nine
 * MP4 files, 34.5MB, on graza.co's homepage alone. Served as
 * application/octet-stream the browser cannot treat them as media — it has no
 * reason to range-request, so it downloads each one in full before playing a
 * frame, and preload="none" cannot help because the response never looks like
 * a video. Sniffing the container recovers the right type without needing the
 * capture to have guessed the extension.
 */
function sniffMime(fp: string, ext: string): string | null {
  if (MIME[ext]) return MIME[ext];
  if (ext !== '.bin' && ext !== '') return null;
  try {
    const head = Buffer.alloc(16);
    const fd = openSync(fp, 'r');
    try { readSync(fd, head, 0, 16, 0); } finally { closeSync(fd); }
    // ISO base media (mp4/m4v/mov): 'ftyp' at offset 4.
    if (head.subarray(4, 8).toString('latin1') === 'ftyp') return 'video/mp4';
    // Matroska/WebM: EBML magic.
    if (head[0] === 0x1a && head[1] === 0x45 && head[2] === 0xdf && head[3] === 0xa3) return 'video/webm';
    if (head.subarray(0, 3).toString('latin1') === 'ID3') return 'audio/mpeg';
    // The bucket is not only media: this capture put a 3.8MB JSON animation
    // in it. Typed as octet-stream a browser will not compress or parse it as
    // text, and some consumers refetch rather than reuse it.
    const lead = head.toString('latin1').trimStart()[0];
    if (lead === '{' || lead === '[') return 'application/json';
  } catch { /* fall through to the generic type */ }
  return null;
}

/**
 * Compress text on the way out.
 *
 * Every real web server does this and this one did not, which is most of why a
 * captured storefront looked so heavy: 6.3MB of JavaScript and 3.8MB of JSON
 * were going over the wire raw. Text of that kind compresses several times
 * over, so the untouched total was never an honest measure of what the page
 * costs a visitor — it was a measure of a missing feature.
 *
 * Only text is worth compressing. Images, fonts and video are already
 * compressed formats; running them through brotli spends CPU to make them
 * marginally larger. Range responses are excluded too — a byte range of a
 * compressed body does not mean what the client asked for.
 *
 * Results are cached because the capture is immutable: the same bytes are
 * served for the life of the process, so compressing once and reusing costs a
 * little memory and saves the work on every later request.
 */
const COMPRESSIBLE = /^(text\/|application\/(javascript|json|xml)|image\/svg\+xml)/;
const compressed = new Map<string, { encoding: string; body: Buffer }>();

function compressFor(
  key: string,
  body: Buffer,
  type: string,
  acceptEncoding: string | null
): { encoding: string; body: Buffer } | null {
  if (!COMPRESSIBLE.test(type)) return null;
  // Not worth a round trip through the compressor, and the header overhead can
  // exceed the saving on very small files.
  if (body.length < 1024) return null;

  const accepts = acceptEncoding ?? '';
  const wantsBrotli = accepts.includes('br');
  const wantsGzip = accepts.includes('gzip');
  if (!wantsBrotli && !wantsGzip) return null;

  const cacheKey = `${key}:${wantsBrotli ? 'br' : 'gzip'}`;
  const hit = compressed.get(cacheKey);
  if (hit) return hit;

  const result = wantsBrotli
    ? {
        encoding: 'br',
        // Quality 5 rather than the default 11: on a multi-megabyte bundle the
        // top setting takes seconds for a few percent, and this runs on first
        // request while someone is waiting for the page.
        body: brotliCompressSync(body, {
          params: { [zlibConstants.BROTLI_PARAM_QUALITY]: 5 }
        })
      }
    : { encoding: 'gzip', body: gzipSync(body, { level: 6 }) };

  compressed.set(cacheKey, result);
  return result;
}

/**
 * The capture is a real storefront, so it still carries the original site's
 * advertising and analytics stack: one page view reached 44 external hosts
 * (Sentry, Klaviyo, Facebook, DoubleClick, several ad exchanges, Shopify
 * telemetry). Three problems with that in a demo:
 *
 *   - it is not offline. The demo phones home from whatever machine it runs
 *     on, which for a client means their network, not ours.
 *   - it is somebody else's telemetry. Those beacons carry the referring URL
 *     and land in the original merchant's analytics.
 *   - it throws. The scripts load in an environment they were not built for
 *     and fail loudly: `fbq is not defined`, `webPixelsManager.createShopify\
 *     Extend is not a function`. Console noise that looks like our bugs.
 *
 * A server-side denylist cannot fix this: those URLs are absolute, so the
 * browser requests them directly and never asks us. CSP can, because the
 * browser enforces it on our behalf before the request leaves the machine.
 *
 * Default-deny, then name the exceptions: self for the capture, and the two
 * payment SDKs that genuinely must load remotely. 'unsafe-inline' and
 * 'unsafe-eval' are required — the captured markup is full of inline
 * handlers we did not write and cannot hash.
 */
// Wildcards, not a hand-listed set. Naming four Stripe hosts looked tidy and
// broke card payment outright: Stripe.js also talks to m.stripe.com,
// r.stripe.com and merchant-ui-api.stripe.com from the parent page, and a
// blocked call there makes confirmPayment fail with no obvious cause. The
// symptom was an order that stayed pending while the webhook log showed
// payment_intent.created and never payment_intent.succeeded.
const PAYMENT_HOSTS = [
  'https://*.stripe.com',
  'https://*.stripe.network',
  'https://*.paypal.com',
  'https://*.paypalobjects.com'
].join(' ');

const CSP = [
  `default-src 'self'`,
  `script-src 'self' 'unsafe-inline' 'unsafe-eval' ${PAYMENT_HOSTS}`,
  `connect-src 'self' ${PAYMENT_HOSTS}`,
  // Stripe and PayPal both mount their card fields in an iframe.
  `frame-src 'self' ${PAYMENT_HOSTS}`,
  `style-src 'self' 'unsafe-inline'`,
  // data: covers inlined SVG in the capture; blob: covers video the page
  // assembles itself.
  `img-src 'self' data: blob:`,
  `media-src 'self' blob:`,
  `font-src 'self' data:`,
  // Nothing in a storefront demo should be submitting a form off-site.
  `form-action 'self' ${PAYMENT_HOSTS}`,
  `base-uri 'self'`,
  `object-src 'none'`
].join('; ');

/**
 * Find the captured file for a request whose name is missing the crawl's
 * content hash: `<dir>/<base>.<ext>` -> `<dir>/<base>.<10-hex>.<ext>`.
 *
 * Cached both ways, including misses. A page that asks for a genuinely absent
 * asset asks for it on every navigation, and re-reading the directory each
 * time turned a missing file into a disk scan per request.
 */
const siblingCache = new Map<string, string | null>();

function resolveHashedSibling(fp: string): string | null {
  if (siblingCache.has(fp)) return siblingCache.get(fp)!;

  let found: string | null = null;
  const file = basename(fp);
  // Look beside the requested path first, then across every asset bucket.
  //
  // The second pass is what rescues a theme built by webpack. Its runtime
  // resolves lazy chunks against __webpack_require__.p, a publicPath baked
  // into the bundle at build time as the merchant's CDN origin. Rewriting the
  // markup cannot reach it: the URL is assembled from a chunk-id table at the
  // moment the chunk is needed. So the page asks for /cdn/shop/t/38/assets/
  // sharedUtils.<themehash>.js while the crawl saved that exact file as
  // _a/js/sharedUtils.<themehash>.<crawlhash>.js.
  //
  // Left unresolved it read as a styling bug, not a missing file. The theme's
  // layout module never ran, so --header-height was never measured, and every
  // h-header / top-header / mt-header utility collapsed to zero: the
  // announcement bar, the wordmark and the cart pill all painted at y=0 on
  // top of each other.
  const dirs = [dirname(fp), ...['js', 'css', 'img', 'font', 'fonts', 'other', 'ext']
    .map((b) => join(siteRoot, '_a', b))];
  const dot = file.lastIndexOf('.');
  if (dot > 0) {
    const base = file.slice(0, dot);
    const ext = file.slice(dot + 1);
    // Anchored on both sides so `app.js` cannot match `app.worker.<hash>.js`.
    const want = new RegExp(
      '^' + base.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\.[0-9a-f]{10}\\.' +
      ext.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '$'
    );
    // An exact name in another bucket counts too: the crawl only appends its
    // hash when the name would otherwise collide.
    for (const d of dirs) {
      if (found || !existsSync(d)) continue;
      for (const name of readdirSync(d)) {
        if (name === file || want.test(name)) { found = join(d, name); break; }
      }
    }
  }
  siblingCache.set(fp, found);
  return found;
}

function serveFile(fp: string, range?: string | null, ifNoneMatch?: string | null, acceptEncoding?: string | null): Response | null {
  if (!existsSync(fp) || statSync(fp).isDirectory()) return null;
  const ext = extname(fp).toLowerCase();
  let body: Buffer | string = readFileSync(fp);
  if (ext === '.html') {
    // Injected at the very top of <head>, deliberately. Two reasons, both
    // learned the hard way: body-level inline scripts are reconciled away by
    // client hydration on React/Hydrogen/Remix storefronts (see commerce.js's
    // header), and the window.FL trap below must run before the capture-time
    // overlay assigns it. Falls back to </body> only if there is no <head>.
    // Injected immediately after <html>, matching crawl.js's injectAtDocStart.
    // That position is the only robust one here: body-level inline scripts are
    // reconciled away by client hydration on React/Hydrogen/Remix storefronts,
    // and the window.FL trap has to run before the capture-time overlay
    // assigns it.
    //
    // The lookahead is load-bearing. `/<head[^>]*>/` also matches `<header>`,
    // and the overlay's own drawer markup contains one inside a JavaScript
    // string — matching it injects a <script> tag into the middle of a string
    // literal and silently kills every script on the page.
    const html = absolutiseScriptPaths(lightenMedia(body.toString('utf8')));
    const at = /<html(?=[\s>])[^>]*>/i;
    body = at.test(html)
      ? html.replace(at, (tag) => tag + SQS_STYLE + SHIM)
      : html.replace(/<\/body>/i, SQS_STYLE + SHIM + '</body>');
  }
  // No-store on HTML. The served markup is rewritten on every request (the
  // bridge shim is injected here, not baked into the capture), so a browser
  // holding a heuristically-cached copy silently serves markup from before the
  // last edit — which is indistinguishable from the injection not working.
  const headers: Record<string, string> = {
    'content-type': sniffMime(fp, ext) || 'application/octet-stream'
  };
  if (ext === '.html') {
    headers['cache-control'] = 'no-store, must-revalidate';
    // A default-deny policy, which does three things at once here.
    //
    // It stops the capture phoning home: a real storefront carries the
    // original merchant's advertising and analytics, and a demo must not
    // beacon to it from a client's machine.
    //
    // It makes the page fast. Those beacons do not fail quickly — they hang
    // until they time out, and the load event waits for them. On graza.co that
    // was the difference between 3051ms and 848ms to load a product page. The
    // sluggishness that felt like a slow backend was the original site's ad
    // stack timing out; the module answers in 35ms warm.
    //
    // And it costs nothing visually. That was worth checking rather than
    // assuming, in both directions: the policy was blamed for missing selected
    // states and a collapsed header, then cleared by rendering the same page
    // with the policy on, off, and bypassed and getting identical output. The
    // remaining visual defects come from the capture, not from this.
    headers['content-security-policy'] = CSP;
  } else {
    // Captured assets are content-addressed — crawl.js writes the file's own
    // hash into the name (graza-fun-fact.d7d3f07cc0.bin), so a given URL can
    // never change meaning. Without a cache header the browser refetched all
    // of them on every navigation: 30MB and 6.9MB of JavaScript re-parsed
    // each time a shopper clicked a link, which is what made browsing a
    // migrated store expensive rather than merely large. One 3.8MB file was
    // being pulled twice within a single page load.
    headers['cache-control'] = 'public, max-age=31536000, immutable';
    // ETag as well, so a client that ignores the above (or revalidates
    // anyway) gets a 304 instead of the body.
    const stat = statSync(fp);
    // Strong, not weak. A weak validator cannot be used to revalidate a range
    // request, so video — the one asset type that always uses ranges, and the
    // largest thing on the page — was refetched in full on every visit. These
    // files are static and content-addressed, so byte-for-byte equality is
    // exactly what the validator can promise.
    headers['etag'] = `"${stat.size.toString(16)}-${Math.floor(stat.mtimeMs).toString(16)}"`;
  }

  // Media has to be range-servable or the correct content-type buys nothing:
  // a browser that cannot ask for byte ranges falls back to fetching the
  // whole file before it can play, which is the behaviour this is trying to
  // avoid. Advertising acceptance and honouring the header lets it stream the
  // few seconds it actually needs.
  if (ifNoneMatch && headers['etag'] && ifNoneMatch === headers['etag']) {
    return new Response(null, { status: 304, headers });
  }

  const type = headers['content-type'];
  if (type.startsWith('video/') || type.startsWith('audio/')) {
    headers['accept-ranges'] = 'bytes';
    const total = (body as Buffer).length;
    if (range) {
      const m = /^bytes=(\d*)-(\d*)$/.exec(range.trim());
      if (m) {
        const start = m[1] ? Number(m[1]) : 0;
        const end = m[2] ? Math.min(Number(m[2]), total - 1) : total - 1;
        if (start <= end && start < total) {
          headers['content-range'] = `bytes ${start}-${end}/${total}`;
          headers['content-length'] = String(end - start + 1);
          return new Response((body as Buffer).subarray(start, end + 1), {
            status: 206,
            headers
          });
        }
      }
    }
  }
  const encoded = compressFor(fp, body as Buffer, type, acceptEncoding ?? null);
  if (encoded) {
    headers['content-encoding'] = encoded.encoding;
    headers['vary'] = 'Accept-Encoding';
    headers['content-length'] = String(encoded.body.length);
    return new Response(encoded.body, { headers });
  }
  return new Response(body, { headers });
}

const productTitles = new Map<string, string>();
async function productTitle(productId?: string): Promise<string> {
  if (!productId) return '';
  const cached = productTitles.get(productId);
  if (cached !== undefined) return cached;
  let title = '';
  try {
    const r = await mod('GET', `/product/${productId}`, `/${productId}`);
    if (r.code === 200) title = r.body?.title ?? '';
  } catch {}
  productTitles.set(productId, title);
  return title;
}

function money(c: number) { return '$' + ((c || 0) / 100).toFixed(2); }

// Product titles come from the scraped catalog, so they are untrusted text —
// they reach the checkout page's markup and must not be able to close a tag or
// an attribute.
function esc(v: unknown): string {
  return String(v ?? '').replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}

Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);
    const p = decodeURIComponent(url.pathname);
    try {
      // ---- Shopify's runtime API, answered from the capture ----
      // /products/<h>.js, /cart.js, ?sections=, recommendations, suggest,
      // telemetry sinks. See shopify-runtime.ts for why a theme needs these.
      const rt = await shopifyRuntime(req, url, p, {
        siteRoot, hasModule: !!moduleUrl, cart: shopifyCart,
        add: async (handle, variantExternalId, quantity) => {
          const variantId = await resolveVariant(handle, variantExternalId);
          if (!variantId) return { item_count: 0, error: 'variant not found' };
          const cid = await ensureCart();
          await mod('POST', '/cart/items', '/items', { cartId: cid, variantId, quantity });
          return shopifyCart();
        },
      });
      if (rt) return rt;

      // ---- storefront -> module bridge ----
      if (p === '/__fl/add' && req.method === 'POST') {
        const { handle, variantExternalId } = await req.json();
        const variantId = await resolveVariant(handle, variantExternalId);
        if (!variantId) return Response.json({ item_count: 0, error: 'variant not found' }, { status: 404 });
        const cid = await ensureCart();
        await mod('POST', '/cart/items', '/items', { cartId: cid, variantId, quantity: 1 });
        return Response.json(await shopifyCart());
      }
      if (p === '/__fl/cart') return Response.json(await shopifyCart());

      // A shopper who CLICKS the cart control lands on /__fl/checkout, because
      // the injected bridge sends them there. A shopper who navigates to /cart
      // directly — from a bookmark, a footer link, or the "Cart" entry in the
      // site nav — used to get a 404, because the crawl never captured a page
      // at that path (Shopify renders /cart server-side from session state, so
      // there is nothing static to capture).
      //
      // Two doors to the same feature that disagree is a defect in itself, and
      // this one is visible: the storefront's own nav offers a link that dead
      // ends. Same destination either way.
      if (p === '/cart' || p === '/cart/') {
        // Browse-only clone (no module): a real cart page from the local
        // cart, never a redirect into a checkout that needs a backend — that
        // was a 500 on every page's cart link, reported as a dead destination.
        if (!MODULE_CONFIGURED) return new Response(browseCartPage(getLocalCart()), { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
        return new Response(null, { status: 302, headers: { location: '/__fl/checkout' } });
      }

      // ---- is the commerce half actually configured? ----------------------
      // Without this, "no ForkLaunch module running" and "the module is broken"
      // look identical from outside: shopifyCart() swallows the connection
      // error and answers `{item_count: 0}`, so an add-to-cart gate sees 0 -> 0
      // and reports FAIL. That is a lie in the expensive direction — it trains
      // whoever reads the report to ignore a red cart line, which is exactly
      // the line that must never be ignored when a real backend IS attached.
      //
      // `configured` is whether a module URL was passed at all (MODULE has a
      // default, so its value proves nothing); `reachable` is whether it
      // answers. A gate can then report SKIP for the first case and FAIL only
      // for the second.
      if (p === '/__fl/health') {
        let reachable = false;
        if (MODULE_CONFIGURED) {
          try {
            const r = await fetch(MODULE + '/health', { signal: AbortSignal.timeout(2500) });
            reachable = r.status < 500;
          } catch {
            // No /health on the module is fine — any answer at all proves it is
            // there. Fall back to a request we know the module implements.
            try {
              const r = await mod('GET', '/product/handle/__fl_probe__', '/handle/__fl_probe__');
              reachable = r.code > 0 && r.code !== 0;
            } catch { reachable = false; }
          }
        }
        return Response.json({
          module: MODULE,
          configured: MODULE_CONFIGURED,
          reachable,
          hmac: !!SECRET,
          stripe: !!STRIPE_PK,
          paypal: !!PAYPAL_CLIENT_ID,
          controls: Object.fromEntries(Object.entries(CONTROLS.kinds || {}).map(([k, v]) => [k, v.length]))
        });
      }
      // Checkout is two steps when a publishable key is present: this page
      // collects the address and card, then posts to /__fl/order below. With
      // no key there is nothing to collect a card with, so keep the original
      // one-shot behaviour and go straight to the order confirmation.
      if (p === '/__fl/checkout') {
        if (!MODULE_CONFIGURED) return new Response(browseCartPage(getLocalCart()), { headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
        if (!STRIPE_PK && !PAYPAL_CLIENT_ID) {
          const cid = await ensureCart();
          const r = await mod('POST', '/checkout', '/', { cartId: cid, provider: 'stripe', shippingAddress: DEMO_ADDRESS });
          // The cart is deliberately kept. The module empties it when the
          // order reaches paid, not when a payment intent is created, so a
          // shopper who leaves the payment step still has their basket.
          return new Response(orderPage(r), { headers: { 'content-type': 'text/html' } });
        }
        const cart = await shopifyCart();
        return new Response(checkoutPage(cart), { headers: { 'content-type': 'text/html' } });
      }

      // Creates the order and its PaymentIntent, handing the client secret to
      // Stripe.js on the checkout page. The charge itself is confirmed in the
      // browser, and the order only becomes `paid` when the provider's webhook
      // reaches the module — never as a side effect of this call.
      if (p === '/__fl/order' && req.method === 'POST') {
        const cid = await ensureCart();
        const { provider: rawProvider, ...address } = await req.json().catch(() => ({}) as any);
        const provider = rawProvider === 'paypal' ? 'paypal' : 'stripe';
        const shippingAddress = { ...DEMO_ADDRESS, ...address };
        const r = await mod('POST', '/checkout', '/', { cartId: cid, provider, shippingAddress });
        if (r.code !== 200) {
          return Response.json({ error: typeof r.body === 'string' ? r.body : (r.body?.message ?? 'checkout failed') }, { status: r.code });
        }
        // Keep the cart id. Dropping it here used to hand the next request a
        // brand-new empty cart, so abandoning checkout and coming back showed
        // an empty bag while a live pending order sat behind it.
        // Stripe gets clientSecret; PayPal gets providerRef, which *is* the
        // PayPal order id and all its JS SDK needs (see checkout.schema.ts).
        return Response.json({
          order: r.body.order,
          clientSecret: r.body.clientSecret,
          providerRef: r.body.payment?.providerRef,
          provider
        });
      }
      // The browser cannot see the capture happen: with PayPal the approval
      // only tells the SDK the buyer said yes, and the money moves later when
      // PayPal's CHECKOUT.ORDER.APPROVED webhook reaches the module. Polling
      // the order is how the page learns the charge actually completed.
      if (p.startsWith('/__fl/order-status/')) {
        const id = p.split('/').pop()!;
        const r = await mod('GET', `/order/${id}`, `/${id}`);
        return Response.json({ status: r.body?.status ?? 'unknown' }, { status: r.code === 200 ? 200 : r.code });
      }

      // ---- static site ----
      let rel = p === '/' ? '/index.html' : p;
      // A module that resolves './script.js' against its own URL asks for
      // <loader.js>/script.js — a path under a file. Serve the sibling.
      const underFile = rel.match(/^(.*)\/[^/]+\.(?:m?js|css)\/([^/]+)$/);
      if (underFile) rel = `${underFile[1]}/${underFile[2]}`;
      if (!extname(rel)) {
        const cand = join(siteRoot, rel.replace(/\/$/, '') + '.html');
        if (existsSync(cand)) rel = rel.replace(/\/$/, '') + '.html';
        else {
          const m = pageFileFor(rel);
          if (m && existsSync(join(siteRoot, m.file))) rel = '/' + m.file;
        }
      }
      const r = serveFile(join(siteRoot, rel), req.headers.get('range'), req.headers.get('if-none-match'), req.headers.get('accept-encoding'));
      if (r) return r;

      // Content-addressed sibling lookup.
      //
      // The crawl saves every asset under a hashed name — chunk.init_X.esm.js
      // becomes chunk.init_X.esm.ae77cba974.js — and rewrites the references
      // it can see: HTML attributes and CSS url(). It cannot see the ones
      // inside JavaScript. An ES module that does `import('./chunk.init_X.esm
      // .js')` resolves that against its own location at runtime, long after
      // any rewrite, and asks for the unhashed name.
      //
      // On graza.co that was 172 dead requests on a single product page. The
      // files were all present; nothing could find them. The theme's own
      // modules never loaded, so the page kept its photography and lost its
      // behaviour — selected states stopped painting and the header collapsed
      // onto itself. It looked like a bad capture and was in fact a naming
      // mismatch.
      //
      // Resolving it here rather than by rewriting the JavaScript is
      // deliberate: a specifier assembled at runtime from variables can never
      // be rewritten statically, and rewriting inside minified bundles risks
      // corrupting them. Matching on the name the page actually asks for
      // catches every case, whatever built it.
      const alias = resolveHashedSibling(join(siteRoot, rel));
      if (alias) {
        const ar = serveFile(alias, req.headers.get('range'), req.headers.get('if-none-match'), req.headers.get('accept-encoding'));
        if (ar) return ar;
      }

      // Query junk welded onto an already-rewritten path.
      //
      // Shopify themes build image URLs by appending transform parameters to a
      // base they were handed: `src = base + '&width=800&crop=center'`, on the
      // assumption that `base` already carries a `?`. The crawl rewrote that
      // base to a local path with no query at all, so the browser asks for
      //
      //   /_a/img/e6dba157-….d863971fa1.jpg&crop=center
      //
      // — a path that has never existed, for a file that is sitting right
      // there. Seven of them on one graza.co product page, and the symptom is
      // simply blank product photography: no error, no console message the
      // page bothers to surface, just holes where the images were.
      //
      // Cutting at the first `&` or `?` recovers the real name. Done here
      // rather than by rewriting the theme's JavaScript for the same reason as
      // the hashed-sibling lookup above: the URL is assembled at runtime from
      // variables and can never be rewritten statically.
      const cut = rel.search(/[&?]/);
      if (cut > 0) {
        const trimmed = rel.slice(0, cut);
        const tr = serveFile(join(siteRoot, trimmed), req.headers.get('range'), req.headers.get('if-none-match'), req.headers.get('accept-encoding'))
          ?? (() => { const a = resolveHashedSibling(join(siteRoot, trimmed));
                      return a ? serveFile(a, req.headers.get('range'), req.headers.get('if-none-match'), req.headers.get('accept-encoding')) : null; })();
        if (tr) return tr;
      }

      // A product page the crawl never reached. The catalog import pulls every
      // product from the source platform's API, while the capture only saves
      // pages the crawler actually walked to, so the two sets always diverge —
      // on graza.co it is 8 captured pages against 79 catalog products. Left
      // alone, most product links in the migrated store are dead.
      //
      // Synthesize the page from the module's own catalog instead. It is the
      // same data the real page renders, so every product in the store stays
      // reachable and buyable regardless of how far the crawl got, on any
      // migrated site.
      const handle = productHandleFromPath(p);
      if (handle) {
        const page = await productFallbackPage(handle);
        if (page) return new Response(page, { headers: { 'content-type': 'text/html' } });
      }
      return new Response('not found', { status: 404 });
    } catch (e: any) {
      return new Response('error: ' + (e?.message || e), { status: 500 });
    }
  },
});

// Present the module cart in Shopify's cart.js shape so the captured theme is happy.
async function shopifyCart() {
  const cid = cartId; if (!cid) return { item_count: 0, items: [], total_price: 0 };
  const r = await mod('GET', `/cart/${cid}`, `/${cid}`);
  // The module's cart carries only variantId + quantity; enrich each line with
  // the variant's price/title so the captured theme's drawer shows real money.
  // Lines are resolved concurrently rather than one after another: a five-line
  // cart used to cost five sequential round trips, so redraw latency grew with
  // cart size. Bounded by the cart's own item count, which the module caps —
  // this is not an unbounded fan-out over a whole catalog.
  const lines = r.body?.items || [];
  const items = await Promise.all(lines.map(async (it: any) => {
    let price = 0, title = 'Item';
    try {
      const cached = variantCache.get(it.variantId);
      const v = cached
        ? { code: 200, body: cached }
        : await mod('GET', `/variant/${it.variantId}`, `/${it.variantId}`);
      if (v.code === 200) {
        if (!cached) variantCache.set(it.variantId, v.body);
        price = v.body.priceCents ?? v.body.price_cents ?? 0;
        // Shopify names the sole variant of a single-variant product
        // "Default Title", which is a placeholder rather than something a
        // shopper should ever read. Lead with the product's own name and
        // append the variant only when it actually distinguishes something.
        const product = await productTitle(v.body.productId);
        const variant = v.body.title && v.body.title !== 'Default Title' ? v.body.title : '';
        title = [product, variant].filter(Boolean).join(' - ') || title;
      }
    } catch {}
    return { quantity: it.quantity, title, price, line_price: price * it.quantity };
  }));
  const count = items.reduce((s: number, i: any) => s + i.quantity, 0);
  const total = items.reduce((s: number, i: any) => s + i.price * i.quantity, 0);
  return { token: 'fl-cart', item_count: count, total_price: total, currency: 'USD', items };
}

/**
 * The card page. Deliberately plain and light — this is injected into somebody
 * else's captured storefront, so it stays neutral rather than imitating a theme
 * it can't reliably match.
 *
 * The browser never sees a secret: it gets the publishable key and a per-order
 * client secret, and Stripe.js posts the card straight to Stripe. The card
 * never touches this process or the module.
 */
/**
 * Shopify serves a product at both /products/<handle> and
 * /collections/<x>/products/<handle>, and the capture may or may not have
 * saved it with a .html suffix — accept every shape so a link from any
 * captured page resolves.
 */
function productHandleFromPath(pathname: string): string | null {
  // Shopify: /products/<handle>, /collections/<c>/products/<handle>.
  // Squarespace: /shop/p/<handle> (any commerce collection: /<collection>/p/<handle>).
  const m =
    /^(?:\/collections\/[^/]+)?\/products\/([^/]+?)(?:\.html)?\/?$/.exec(pathname) ||
    /^\/[^/]+\/p\/([^/]+?)(?:\.html)?\/?$/.exec(pathname) ||
    /^\/[^/]+\/p-([^/]+?)\.html$/.exec(pathname); // the capture's flattened file name for the same page
  return m ? decodeURIComponent(m[1]) : null;
}

/**
 * Renders a product page from the module's catalog for products the crawl
 * never captured. Deliberately plain: this stands in for an arbitrary
 * merchant's theme, and a neutral page that works beats a guessed imitation
 * that doesn't. Returns null when the handle isn't in the catalog either, so
 * the caller can fall through to a real 404.
 */
/** Cart page for the browse-only clone: what is in the basket, and why checkout stops here. */
function browseCartPage(cart: any): string {
  const e = (t: any) => String(t ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c] as string));
  const money = (c: number) => '$' + (Number(c || 0) / 100).toFixed(2);
  const rows = (cart.items || []).map((i: any) => `<li style="display:flex;gap:14px;align-items:center;padding:12px 0;border-bottom:1px solid #eee;list-style:none">${i.image ? `<img src="${e(i.image)}" alt="" width="64" height="64" style="object-fit:cover;border-radius:6px">` : ''}<span style="flex:1"><strong>${e(i.product_title || i.title)}</strong>${i.variant_title ? `<br><span style="color:#666">${e(i.variant_title)}</span>` : ''}</span><span>× ${e(i.quantity)}</span><span style="min-width:80px;text-align:right">${money(i.line_price)}</span></li>`).join('');
  return `<!doctype html><html><head><meta charset="utf-8"><title>Your cart</title><meta name="viewport" content="width=device-width,initial-scale=1"></head><body style="font:16px/1.4 system-ui,sans-serif;max-width:720px;margin:40px auto;padding:0 20px"><h1 style="font-size:24px">Your cart</h1>${rows ? `<ul style="padding:0">${rows}</ul><p style="text-align:right;font-size:18px"><strong>Subtotal ${money(cart.total_price)}</strong></p>` : '<p>Your cart is empty.</p>'}<p style="background:#f6f6f6;padding:12px 14px;border-radius:8px;color:#444">This is a browse-only clone: the cart works, checkout is not wired. Run the migration with <code>--server</code> to connect the ForkLaunch ecommerce module and take real orders.</p><p><a href="/">← Continue shopping</a></p></body></html>`;
}

async function productFallbackPage(handle: string): Promise<string | null> {
  const p = await mod('GET', `/product/handle/${handle}`, `/handle/${handle}`);
  if (p.code !== 200 || !p.body?.id) return null;
  const vs = await mod('GET', `/variant/product/${p.body.id}`, `/product/${p.body.id}`);
  const variants: any[] = vs.code === 200 && Array.isArray(vs.body) ? vs.body : [];
  if (!variants.length) return null;

  // Images stay on the source CDN: the capture only localises assets for pages
  // it actually saved, so there is no local copy for this product. The page
  // still works without them.
  const image = (p.body.images ?? [])[0]?.src ?? '';
  const opts = variants
    .map((v, i) => `<option value="${esc(v.externalId)}"${i ? '' : ' selected'}>${esc(
      v.title && v.title !== 'Default Title' ? v.title : p.body.title
    )} - ${money(v.priceCents)}</option>`)
    .join('');

  return `<!doctype html><html><head><meta charset=utf-8><title>${esc(p.body.title)}</title>
<meta name=viewport content="width=device-width,initial-scale=1"><style>
 body{font:16px/1.6 system-ui,-apple-system,Segoe UI,sans-serif;color:#111;margin:0;background:#fff}
 .w{max-width:940px;margin:0 auto;padding:40px 24px;display:grid;grid-template-columns:1fr 1fr;gap:44px}
 @media(max-width:720px){.w{grid-template-columns:1fr;gap:24px}}
 img{width:100%;border-radius:12px;background:#f4f4f4;display:block}
 h1{font-size:1.9rem;margin:0 0 6px}.pr{font-size:1.3rem;margin:0 0 20px}
 select,button{width:100%;font:15px inherit;border-radius:8px;padding:12px;box-sizing:border-box}
 select{border:1px solid #ddd;margin-bottom:12px;background:#fff}
 button{background:#111;color:#fff;border:0;font-weight:600;cursor:pointer}
 .d{margin-top:26px;color:#444;font-size:.95rem}
 .n{margin-top:28px;padding:11px 13px;border:1px dashed #ddd;border-radius:8px;color:#888;font-size:.8rem}
 a{color:#111}
</style></head><body><div class=w>
 <div>${image ? `<img src="${esc(image)}" alt="${esc(p.body.title)}">` : ''}</div>
 <div>
  <h1>${esc(p.body.title)}</h1>
  <p class=pr>${money(variants[0].priceCents)}</p>
  <select id=v>${opts}</select>
  <button id=add>Add to cart</button>
  <div class=d>${p.body.descriptionHtml ?? ''}</div>
  <p style="margin-top:24px"><a href="/">&larr; Back to the store</a></p>
 </div></div>
<script>
document.getElementById('add').onclick=async()=>{
 const b=document.getElementById('add');b.disabled=true;b.textContent='Adding...';
 try{
  const r=await fetch('/__fl/add',{method:'POST',headers:{'content-type':'application/json'},
   body:JSON.stringify({handle:${JSON.stringify(handle)},variantExternalId:document.getElementById('v').value})});
  if(!r.ok) throw new Error('add failed');
  b.textContent='Added - go to checkout';
  b.onclick=()=>location.href='/__fl/checkout';b.disabled=false;
 }catch(e){b.textContent='Unavailable';setTimeout(()=>{b.textContent='Add to cart';b.disabled=false},1600);}
};
</script></body></html>`;
}

/**
 * Only offers providers this server was actually given credentials for, and
 * renders nothing when there is just one — a radio group with a single option
 * is noise.
 */
function methodChooser(): string {
  if (!STRIPE_PK || !PAYPAL_CLIENT_ID) return '';
  return `<div style="margin:4px 0 14px">
    <label style="display:flex;align-items:center;gap:9px;margin-bottom:7px;font-size:.95rem;color:#111">
      <input type=radio name=prov value=stripe checked style="width:auto"> Credit or debit card</label>
    <label style="display:flex;align-items:center;gap:9px;font-size:.95rem;color:#111">
      <input type=radio name=prov value=paypal style="width:auto"> PayPal</label>
  </div>`;
}

function checkoutPage(cart: { items: any[]; total_price: number }): string {
  const lines = (cart.items || [])
    .map((i) => `<div class=l><span>${esc(i.title)} x${i.quantity}</span><span>${money(i.line_price)}</span></div>`)
    .join('');
  return `<!doctype html><html><head><meta charset=utf-8><title>Checkout</title>
<meta name=viewport content="width=device-width,initial-scale=1">
<script src="https://js.stripe.com/v3/"></script>
<style>
 body{font:16px/1.6 system-ui,-apple-system,Segoe UI,sans-serif;color:#111;background:#fff;margin:0}
 .wrap{max-width:900px;margin:0 auto;padding:40px 24px 80px;display:grid;grid-template-columns:1fr 300px;gap:36px;align-items:start}
 @media(max-width:760px){.wrap{grid-template-columns:1fr}}
 h1{font-size:1.6rem;margin:0 0 4px}.sub{color:#777;font-size:.9rem;margin:0 0 26px}
 .box{border:1px solid #e6e6e6;border-radius:12px;padding:22px}.box+.box{margin-top:16px}
 h2{font-size:.72rem;letter-spacing:.12em;text-transform:uppercase;color:#888;margin:0 0 16px}
 label{display:block;font-size:.75rem;color:#777;margin:0 0 5px}
 input{width:100%;box-sizing:border-box;border:1px solid #ddd;border-radius:7px;padding:10px 12px;font:15px inherit}
 input:focus{outline:none;border-color:#111}
 .f{margin-bottom:12px}.r3{display:grid;grid-template-columns:2fr 1fr 1fr;gap:10px}
 button{width:100%;background:#111;color:#fff;border:0;border-radius:7px;padding:14px;font:600 15px inherit;cursor:pointer}
 button:disabled{background:#bbb;cursor:not-allowed}
 .l{display:flex;justify-content:space-between;padding:6px 0;font-size:.9rem}
 .t{border-top:1px solid #eee;margin-top:10px;padding-top:12px;font-weight:700}
 .err{color:#c00;font-size:.85rem;min-height:18px;margin-top:10px}
 .step{opacity:.4;pointer-events:none}.step.on{opacity:1;pointer-events:auto}
 .note{font:12px ui-monospace,Menlo,monospace;color:#888;border:1px dashed #ddd;border-radius:6px;padding:9px 11px;margin-top:12px}
 .ok{text-align:center;padding:70px 20px}
 .ok .tick{width:62px;height:62px;border-radius:50%;background:#111;color:#fff;font-size:30px;line-height:62px;margin:0 auto 18px}
</style></head><body><div class=wrap id=main>
 <div>
  <h1>Checkout</h1><p class=sub>All transactions are secure and encrypted.</p>
  <div class="box step on" id=s1>
   <h2>Shipping address</h2>
   <div class=f><label>Full name</label><input id=name value="${esc(DEMO_ADDRESS.name)}"></div>
   <div class=f><label>Address</label><input id=line1 value="${esc(DEMO_ADDRESS.line1)}"></div>
   <div class=r3>
    <div class=f><label>City</label><input id=city value="${esc(DEMO_ADDRESS.city)}"></div>
    <div class=f><label>State</label><input id=state value="${esc(DEMO_ADDRESS.state)}"></div>
    <div class=f><label>ZIP</label><input id=postalCode value="${esc(DEMO_ADDRESS.postalCode)}"></div>
   </div>
   ${methodChooser()}
   <button id=go>Continue to payment</button>
  </div>
  <div class="box step" id=s2>
   <h2>Payment</h2>
   <div id=pe></div>
   <div id=pp></div>
   <div class=err id=err></div>
   <div class=note id=hint style="display:none"></div>
   <button id=pay style="margin-top:14px">Pay</button>
  </div>
 </div>
 <div class=box id=sum><h2>Order summary</h2>${lines || '<div style="color:#888">Your cart is empty.</div>'}
  <div class="l t"><span>Subtotal</span><span>${money(cart.total_price)}</span></div>
  <div class=l style="color:#888"><span>Tax &amp; shipping</span><span>at payment</span></div>
 </div></div>
<script>
const $=i=>document.getElementById(i);
const PK=${JSON.stringify(STRIPE_PK)}, PPID=${JSON.stringify(PAYPAL_CLIENT_ID)};
const stripe=PK?Stripe(PK):null;
let elements,orderId;
const m=c=>'$'+(c/100).toFixed(2);
const chosen=()=>{const r=document.querySelector('input[name=prov]:checked');
 return r?r.value:(PK?'stripe':'paypal');};

// PayPal's capture is done by the module when PayPal's approval webhook lands,
// never by this page — calling actions.order.capture() here would charge the
// buyer a second time. So after approval we watch the order instead.
function loadPaypal(){return new Promise((ok,no)=>{
 if(window.paypal) return ok();
 const sc=document.createElement('script');
 sc.src='https://www.paypal.com/sdk/js?client-id='+encodeURIComponent(PPID)+'&currency=USD&intent=capture';
 sc.onload=ok; sc.onerror=()=>no(new Error('PayPal SDK failed to load'));
 document.head.appendChild(sc);});}

async function waitForPaid(who){
 who=who||'The provider';
 for(let i=0;i<40;i++){
  try{const r=await fetch('/__fl/order-status/'+orderId);
   const d=await r.json();
   if(d.status==='paid'){done('Payment received',who+' captured the charge and the module marked the order paid.');return;}
   if(d.status==='cancelled'){done('Payment failed',who+' declined the capture.');return;}
  }catch(e){}
  await new Promise(r=>setTimeout(r,1500));
 }
 done('Approved, awaiting confirmation',
  who+' has the approval. The order moves to paid when the webhook reaches the module - if it is still pending, check that the tunnel is up and the approval event is subscribed.');
}

function done(title,msg){
 $('main').innerHTML='<div class=ok style="grid-column:1/-1"><div class=tick>&#10003;</div>'+
  '<h1>'+title+'</h1><p class=sub>'+msg+'</p>'+
  '<p style="color:#888">Order '+(orderId||'')+'</p>'+
  '<p style="margin-top:26px"><a href="/">&larr; Continue shopping</a></p></div>';
}
// Coming back from a redirect payment method. Stripe returns the shopper to
// return_url with payment_intent_client_secret and redirect_status appended.
// Without this branch the page just rebuilt an empty checkout and the shopper
// saw no acknowledgement of a payment they had already authorised.
(function resumeFromRedirect(){
 var q=new URLSearchParams(location.search);
 var secret=q.get('payment_intent_client_secret');
 if(!secret||!stripe) return;
 orderId=q.get('fl_order')||'';
 // Ask Stripe rather than trusting redirect_status in the URL: the shopper
 // controls the address bar, and money must never be confirmed from it.
 stripe.retrievePaymentIntent(secret).then(function(res){
  var pi=res&&res.paymentIntent;
  var st=pi&&pi.status;
  if(st==='succeeded'||st==='processing'){
   done('Payment authorised','Waiting for the provider to confirm. The module marks the order paid when the webhook arrives.');
   waitForPaid('Stripe');
  }else if(st==='requires_payment_method'){
   done('Payment not completed','That payment method was declined or cancelled. Your bag is untouched, so you can try again.');
  }else{
   done('Payment status unknown','Stripe reports "'+String(st)+'". The order stays pending until a webhook resolves it.');
  }
 }).catch(function(e){$('err').textContent=String(e&&e.message||e);});
})();

$('go').onclick=async()=>{
 const b=$('go');b.disabled=true;b.textContent='Creating order...';
 try{
  const prov=chosen();
  const res=await fetch('/__fl/order',{method:'POST',headers:{'content-type':'application/json'},
   body:JSON.stringify({provider:prov,name:$('name').value,line1:$('line1').value,city:$('city').value,
    state:$('state').value,postalCode:$('postalCode').value,country:'US'})});
  const d=await res.json();
  if(!res.ok) throw new Error(d.error||('checkout '+res.status));
  orderId=d.order&&d.order.id;
  const o=d.order;
  $('sum').innerHTML='<h2>Order summary</h2>'+
   '<div class=l><span>Subtotal</span><span>'+m(o.subtotalCents)+'</span></div>'+
   '<div class=l><span>Tax</span><span>'+m(o.taxCents)+'</span></div>'+
   '<div class=l><span>Shipping</span><span>'+(o.shippingCents?m(o.shippingCents):'Free')+'</span></div>'+
   '<div class="l t"><span>Total</span><span>'+m(o.totalCents)+'</span></div>';
  $('s1').classList.remove('on');$('s2').classList.add('on');
  b.textContent='Shipping to '+$('city').value+' ✓';

  if(prov==='paypal'){
   if(!d.providerRef) throw new Error('no PayPal order id - are PAYPAL_CLIENT_ID/SECRET set on the module?');
   $('pay').style.display='none';
   $('hint').style.display='none';
   await loadPaypal();
   paypal.Buttons({
    // The order already exists in the module; hand its id straight to the SDK
    // rather than creating a second one.
    createOrder:()=>d.providerRef,
    // Deliberately no actions.order.capture(): the module captures when
    // PayPal's approval webhook arrives. Capturing here would double-charge.
    onApprove:()=>{$('pp').innerHTML='<p style="color:#888">Approved. Waiting for PayPal to confirm the capture...</p>';return waitForPaid('PayPal');},
    onError:(e)=>{$('err').textContent=String(e&&e.message||e);}
   }).render('#pp');
  }else{
   if(!d.clientSecret) throw new Error('no client secret - is STRIPE_API_KEY set on the module?');
   elements=stripe.elements({clientSecret:d.clientSecret});
   elements.create('payment',{layout:'tabs'}).mount('#pe');
   $('pay').textContent='Pay '+m(o.totalCents);
  }
  $('s2').scrollIntoView({behavior:'smooth',block:'start'});
 }catch(e){$('err').textContent=e.message;b.disabled=false;b.textContent='Continue to payment';}
};
$('pay').onclick=async()=>{
 const b=$('pay');b.disabled=true;b.textContent='Processing...';$('err').textContent='';
 // return_url is required as soon as the shopper picks a redirect-based
 // method (Affirm, Cash App Pay, bank); Stripe rejects the confirmation
 // without it even though card payments stay inline and never navigate.
 const {error}=await stripe.confirmPayment({elements,redirect:'if_required',
   confirmParams:{return_url:location.origin+'/__fl/checkout?fl_order='+encodeURIComponent(orderId||'')}});
 if(error){$('err').textContent=error.message;b.disabled=false;b.textContent='Pay';return;}
 done('Payment received','Stripe confirmed the charge. The module marks the order paid '+
  'when the webhook arrives, and the worker adjusts inventory.');
};
</script></body></html>`;
}

function orderPage(r: { code: number; body: any }): string {
  const ok = r.code === 200;
  const order = r.body?.order || r.body;
  const created = r.code === 200 || r.code === 502; // 502 = order created, payment (Stripe) not initiated
  const status = order?.status || (r.code === 502 ? 'created (payment pending — Stripe key needed)' : 'error');
  const oid = order?.id || (typeof r.body === 'string' ? (r.body.match(/[0-9a-f-]{36}/)?.[0] ?? '') : '');
  const total = order?.totalCents != null ? money(order.totalCents) : '';
  return `<!doctype html><html><head><meta charset=utf-8><title>Order — ForkLaunch</title></head>
<body style="font:16px/1.6 system-ui;max-width:640px;margin:60px auto;padding:0 24px;color:#111">
  <div style="width:64px;height:64px;border-radius:50%;background:${created ? '#111' : '#c00'};color:#fff;font-size:32px;line-height:64px;text-align:center;margin:0 auto 20px">${created ? '✓' : '×'}</div>
  <h1 style="text-align:center">${created ? 'Order placed on ForkLaunch' : 'Checkout error'}</h1>
  <div style="background:#fafafa;border:1px solid #eee;border-radius:12px;padding:20px;margin-top:24px">
    <div><b>Order</b>: ${oid || '—'}</div>
    <div><b>Status</b>: ${status}</div>
    ${total ? `<div><b>Total</b>: ${total}</div>` : ''}
    <div style="margin-top:10px;color:#888;font-size:13px">Backed by the ForkLaunch ecommerce module${ok ? '' : ' — add a real STRIPE_API_KEY to complete the card charge'}.</div>
  </div>
  <p style="text-align:center;margin-top:24px"><a href="/">← Continue shopping</a></p>
</body></html>`;
}

// Fail loudly if the shim does not parse. It is assembled as a template
// literal, which quietly eats a backslash — so a regex written \s becomes s —
// and a stray backtick truncates the script entirely. Both have happened, and
// both times the markup still contained a plausible-looking <script> while
// every page was inert: the store looked fine, served 200s, and silently did
// nothing. Parsing it once at startup turns that into an immediate, obvious
// failure instead of a silent one discovered by hand later.
try {
  const body = SHIM.replace(/^<script>/i, '').replace(/<\/script\s*>$/i, '');
  new Function(body);
} catch (err) {
  console.error('FATAL: the injected client shim does not parse — every page would load inert.');
  console.error(String(err));
  process.exit(1);
}

console.log(MODULE_CONFIGURED
  ? `storefront (wired to ForkLaunch module ${MODULE}) on http://localhost:${PORT}`
  : `storefront (browse-only: no ForkLaunch module wired; cart works, checkout stops) on http://localhost:${PORT}`);
