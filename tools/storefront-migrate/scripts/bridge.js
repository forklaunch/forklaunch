/**
 * Commerce bridge — the piece that turns a captured storefront into a store.
 *
 * A captured storefront still talks to Shopify's AJAX Cart API (/cart/add.js,
 * /cart.js, /cart/update.js) and its search/product endpoints. Offline nothing
 * answers those, so every button that isn't a plain link does nothing. That is
 * the difference between a clone that *looks* right and one that *works*.
 *
 * This injects a runtime shim that patches fetch + XMLHttpRequest and answers
 * those calls. Two modes:
 *
 *   backend mode (--api <url>)  — proxy to the ForkLaunch ecommerce module
 *   local mode   (default)      — an in-browser cart, persisted to
 *                                 localStorage, that speaks Shopify's response
 *                                 shape so the storefront's own JS is satisfied
 *
 * Local mode exists so add-to-cart genuinely works before the module's cart
 * controller has landed. When the backend is live, the same storefront points
 * at it by passing --api; nothing else changes.
 *
 * Exported as a function so crawl.js can inline it per page.
 */

function buildBridge({ apiBase = null, catalog = null, hmacSecret = null } = {}) {
  const cfg = JSON.stringify({ apiBase, catalog: catalog || null, hmacSecret: hmacSecret || null });

  // NOTE: kept as one string so it can be inlined into the captured HTML.
  return '<script>(function(){\n' +
    'var CFG=' + cfg + ';\n' +
    'var KEY="_fl_cart";\n' +
    // ---- cart state -------------------------------------------------------
    'function load(){try{return JSON.parse(localStorage.getItem(KEY))||{items:[]}}catch(e){return {items:[]}}}\n' +
    'function save(c){try{localStorage.setItem(KEY,JSON.stringify(c))}catch(e){}}\n' +
    'function money(c){return (c/100).toFixed(2)}\n' +
    // Shopify's cart payload shape — the storefront JS reads these fields, so
    // the response has to match or the UI silently fails to update.
    'function payload(c){var total=0,n=0;\n' +
    '  var items=c.items.map(function(i){var line=i.price*i.quantity;total+=line;n+=i.quantity;\n' +
    '    return {id:i.id,key:String(i.id),product_id:i.product_id||i.id,variant_id:i.id,\n' +
    '      title:i.title,product_title:i.title,quantity:i.quantity,price:i.price,\n' +
    '      line_price:line,final_price:i.price,final_line_price:line,\n' +
    '      image:i.image||null,url:i.url||"#",variant_title:i.variant_title||null};});\n' +
    '  return {token:"local",item_count:n,items:items,total_price:total,\n' +
    '    original_total_price:total,items_subtotal_price:total,currency:"USD",\n' +
    '    requires_shipping:true};}\n' +
    // ---- backend proxy ----------------------------------------------------
    'function apiUrl(path){return CFG.apiBase? CFG.apiBase.replace(/\\/$/,"")+path : null;}\n' +
    // ---- request handling -------------------------------------------------
    'function jsonRes(obj){return new Response(JSON.stringify(obj),\n' +
    '  {status:200,headers:{"Content-Type":"application/json"}});}\n' +
    'function parseBody(body){try{\n' +
    '  if(!body)return {};\n' +
    '  if(typeof body==="string"){try{return JSON.parse(body)}catch(e){\n' +
    '    var o={},sp=new URLSearchParams(body);sp.forEach(function(v,k){o[k]=v});return o;}}\n' +
    '  if(body instanceof FormData){var o={};body.forEach(function(v,k){o[k]=v});return o;}\n' +
    '  return body;}catch(e){return {}}}\n' +
    'function handle(path,method,body){\n' +
    '  var c=load();\n' +
    '  if(/\\/cart\\/add(\\.js)?$/.test(path)){\n' +
    '    var b=parseBody(body);\n' +
    '    var id=b.id||b.variant_id||(b.items&&b.items[0]&&b.items[0].id);\n' +
    '    var qty=parseInt(b.quantity||(b.items&&b.items[0]&&b.items[0].quantity)||1,10);\n' +
    '    if(id){var found=null;\n' +
    '      c.items.forEach(function(i){if(String(i.id)===String(id))found=i});\n' +
    '      if(found)found.quantity+=qty;\n' +
    '      else c.items.push({id:id,title:(b.title||document.title||"Item"),\n' +
    '        price:parseInt(b.price||0,10)||0,quantity:qty});\n' +
    '      save(c);}\n' +
    '    return payload(c);}\n' +
    '  if(/\\/cart\\/change(\\.js)?$/.test(path)||/\\/cart\\/update(\\.js)?$/.test(path)){\n' +
    '    var b2=parseBody(body);\n' +
    '    if(b2.updates){Object.keys(b2.updates).forEach(function(k){\n' +
    '      c.items=c.items.filter(function(i){\n' +
    '        if(String(i.id)!==String(k))return true;\n' +
    '        i.quantity=parseInt(b2.updates[k],10);return i.quantity>0;});});}\n' +
    '    else if(b2.id!==undefined){c.items=c.items.filter(function(i){\n' +
    '      if(String(i.id)!==String(b2.id))return true;\n' +
    '      i.quantity=parseInt(b2.quantity,10);return i.quantity>0;});}\n' +
    '    save(c);return payload(c);}\n' +
    '  if(/\\/cart\\/clear(\\.js)?$/.test(path)){save({items:[]});return payload({items:[]});}\n' +
    '  if(/\\/cart(\\.js)?$/.test(path))return payload(c);\n' +
    '  if(/\\/search\\/suggest/.test(path))return {resources:{results:{products:[]}}};\n' +
    '  return null;}\n' +
    // ---- patch fetch ------------------------------------------------------
    'var _f=window.fetch;\n' +
    'window.fetch=function(input,init){\n' +
    '  try{\n' +
    '    var url=typeof input==="string"?input:(input&&input.url)||"";\n' +
    '    var m=((init&&init.method)||(input&&input.method)||"GET").toUpperCase();\n' +
    '    var path=url.replace(/^https?:\\/\\/[^\\/]+/,"").split("?")[0];\n' +
    '    if(/^\\/(cart|search)/.test(path)){\n' +
    '      var api=apiUrl(path);\n' +
    '      if(api)return _f(api,init);\n' +
    '      var r=handle(path,m,(init&&init.body));\n' +
    '      if(r)return Promise.resolve(jsonRes(r));}\n' +
    '  }catch(e){}\n' +
    '  return _f.apply(window,arguments);};\n' +
    // ---- patch XHR --------------------------------------------------------
    'var _o=XMLHttpRequest.prototype.open,_s=XMLHttpRequest.prototype.send;\n' +
    'XMLHttpRequest.prototype.open=function(m,u){this._m=m;this._u=u;\n' +
    '  return _o.apply(this,arguments);};\n' +
    'XMLHttpRequest.prototype.send=function(body){\n' +
    '  try{var path=String(this._u||"").replace(/^https?:\\/\\/[^\\/]+/,"").split("?")[0];\n' +
    '    if(/^\\/(cart|search)/.test(path)&&!CFG.apiBase){\n' +
    '      var r=handle(path,(this._m||"GET").toUpperCase(),body);\n' +
    '      if(r){var self=this;\n' +
    '        Object.defineProperty(self,"readyState",{value:4,configurable:true});\n' +
    '        Object.defineProperty(self,"status",{value:200,configurable:true});\n' +
    '        Object.defineProperty(self,"responseText",{value:JSON.stringify(r),configurable:true});\n' +
    '        Object.defineProperty(self,"response",{value:JSON.stringify(r),configurable:true});\n' +
    '        setTimeout(function(){\n' +
    '          if(self.onreadystatechange)self.onreadystatechange();\n' +
    '          if(self.onload)self.onload();\n' +
    '          self.dispatchEvent(new Event("load"));},10);\n' +
    '        return;}}\n' +
    '  }catch(e){}\n' +
    '  return _s.apply(this,arguments);};\n' +
    // ---- surface the mode so it is never ambiguous what is running --------
    'window.__forklaunchBridge={mode:CFG.apiBase?"backend":"local",api:CFG.apiBase};\n' +
    // ---- filters / sort / search ------------------------------------------
    // Layered on top of everything above: patches window.fetch a second
    // time (chaining, not replacing, the cart patch already installed) and
    // wires DOM events so a captured collection page's own filter/sort/
    // search controls actually change the rendered grid, backed by the
    // real ecommerce module's GET /product (and GET /variant for price —
    // Product itself carries no price, that's variant-level). See
    // filtersMain's internal comments for exactly what is honestly backed
    // vs. applied client-side vs. left disabled. Entirely inert — early
    // return, no listeners attached — when CFG.apiBase is not set, so
    // local-mode behavior is provably unchanged.
    ';(' + filtersMain.toString() + ')(CFG, apiUrl, jsonRes);\n' +
    '})();</' + 'script>';
}

/**
 * Everything needed to answer a captured storefront's filter/sort/search
 * controls from the real ecommerce module, inlined into the page via
 * Function.prototype.toString() (see buildBridge above) rather than
 * hand-escaped string concatenation — this is a few hundred lines and
 * doing that as a manual string would be unreviewable and impossible to
 * keep correct.
 *
 * Contract this maps onto (blueprint/ecommerce-stripe/api/controllers/
 * product.controller.ts, READ FIRST — this is not guesswork):
 *   GET /product?ids&title&minPriceCents&maxPriceCents&inStock&optionName&optionValue
 *     - `title` is a case-insensitive substring match (product.service.ts:
 *       `$ilike %title%`) against the product title ONLY — not description,
 *       vendor, or tags.
 *     - `optionName`/`optionValue` is a SINGLE pair — the API cannot narrow
 *       by two different option facets (e.g. color AND size) at once.
 *     - There is no sort parameter and no pagination at all.
 *   GET /variant (no query) returns every variant — used once per page to
 *     build a productId -> variants[] index for price/compare-at, since
 *     Product has no price of its own.
 *
 * What that means for Shopify's filter/sort/search vocabulary:
 *   - filter.v.price.gte/lte  -> minPriceCents/maxPriceCents (server-side,
 *     genuinely backed; Shopify's values are plain dollars, so *100).
 *   - filter.v.availability   -> inStock=true (server-side, genuinely
 *     backed; unchecked never sends inStock=false, matching Shopify's own
 *     "in stock only" semantics).
 *   - filter.v.option.<name>  -> optionName/optionValue (server-side for
 *     the FIRST option facet the shopper has active values for, unioned
 *     across multiple selected values with parallel calls since the API
 *     only takes one value per call; any ADDITIONAL option facets, and
 *     filter.p.product_type, are narrowed CLIENT-SIDE over that result
 *     using the real variant/product data already fetched — honest, just
 *     not server-pushed).
 *   - sort_by                 -> entirely client-side (no server support):
 *     title-ascending/descending, price-ascending/descending, created-
 *     ascending/descending are real sorts over real data. best-selling,
 *     manual, and most-relevant have no backing data (no sales/relevance
 *     signal in the module) and are left DISABLED in the <select>/radio
 *     controls, not silently ignored.
 *   - filter.p.m.* (theme metafield filters — flavor, roast, material,
 *     etc.) -> NO backing data at all (Product carries no metafields in
 *     this schema). Disabled at init, not wired to any handler.
 *   - q / /search/suggest.json -> title substring match, reshaped into
 *     Shopify's predictive-search JSON so the theme's own dropdown remains
 *     the presentation layer. The full /search results PAGE is not handled
 *     — crawl.js does not currently capture a static page for it, so
 *     there's nothing to render into; documented as a known gap, not faked.
 */
function filtersMain(CFG, apiUrl, jsonRes) {
  if (!CFG.apiBase) return; // local mode: zero behavior change, by design.

  var SIGNED_PATH = '/'; // GET /product and GET /variant are both mounted
  // at their router's root ('/product', '/variant' resp.) — the framework
  // signs/verifies HMAC over the path *relative to that mount*, always '/'
  // for these two calls. Confirmed against
  // ecommerce-stripe/__test__/test-utils.ts (signTestRequest) and
  // ecommerce-stripe/api/routes/{product,variant}.routes.ts, not guessed.

  var SUPPORTED_SORTS = {
    'title-ascending': 1,
    'title-descending': 1,
    'price-ascending': 1,
    'price-descending': 1,
    'created-ascending': 1,
    'created-descending': 1
  };

  // ---- HMAC (matches framework/core/src/http/createHmacToken.ts) --------
  function uuidv4() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (c) {
      var r = (Math.random() * 16) | 0;
      var v = c === 'x' ? r : (r & 0x3) | 0x8;
      return v.toString(16);
    });
  }

  function b64FromBuffer(buf) {
    var bytes = new Uint8Array(buf);
    var bin = '';
    for (var i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
    return btoa(bin);
  }

  function signHmacGet(secret, signedPath) {
    if (!(window.crypto && window.crypto.subtle)) {
      return Promise.reject(new Error(
        'forklaunch bridge: crypto.subtle unavailable — HMAC signing needs a ' +
        'secure context (https://, or http://localhost / http://127.0.0.1). ' +
        'Serve this capture from localhost/127.0.0.1.'
      ));
    }
    var ts = new Date().toISOString();
    var nonce = uuidv4();
    // The framework's own signer (createHmacToken.ts) embeds the LITERAL
    // string "undefined" in the signed message for a bodyless request —
    // a template-literal quirk (`${bodyString}` where bodyString is the JS
    // value `undefined`, not the empty string), not a stylistic choice.
    // Signing '' instead produces a signature that fails with the exact
    // same 403 as a wrong secret — verified against the real module's
    // auth.middleware.ts / discriminateAuthMethod.ts and reproduced via
    // ecommerce-stripe/__test__/test-utils.ts's own documented finding.
    var msg = 'GET\n' + signedPath + '\nundefined' + ts + '\n' + nonce;
    return window.crypto.subtle
      .importKey('raw', new TextEncoder().encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign'])
      .then(function (key) {
        return window.crypto.subtle.sign('HMAC', key, new TextEncoder().encode(msg));
      })
      .then(function (sigBuf) {
        return 'HMAC keyId=default ts=' + ts + ' nonce=' + nonce + ' signature=' + b64FromBuffer(sigBuf);
      });
  }

  // ---- API -----------------------------------------------------------
  function qs(params) {
    var parts = [];
    Object.keys(params || {}).forEach(function (k) {
      var v = params[k];
      if (v === null || v === undefined || v === '') return;
      parts.push(encodeURIComponent(k) + '=' + encodeURIComponent(v));
    });
    return parts.join('&');
  }

  function apiGet(path, params) {
    var query = qs(params);
    var url = CFG.apiBase.replace(/\/$/, '') + path + (query ? '?' + query : '');
    var authP = CFG.hmacSecret
      ? signHmacGet(CFG.hmacSecret, SIGNED_PATH)
      : Promise.resolve(null);
    return authP
      .then(function (auth) {
        var headers = {};
        if (auth) headers.authorization = auth;
        return fetch(url, { headers: headers });
      })
      .then(function (res) {
        if (!res.ok) {
          return res.text().then(function (txt) {
            throw new Error('forklaunch bridge: ' + path + ' -> ' + res.status + ' ' + txt);
          });
        }
        return res.json();
      });
  }

  var variantsByProductPromise = null;
  function allVariantsByProduct() {
    if (!variantsByProductPromise) {
      variantsByProductPromise = apiGet('/variant', {})
        .then(function (list) {
          var byProduct = {};
          (list || []).forEach(function (v) {
            (byProduct[v.productId] = byProduct[v.productId] || []).push(v);
          });
          return byProduct;
        })
        .catch(function (e) {
          console.error('[forklaunch bridge] failed to load variants (price/stock will be unavailable)', e);
          variantsByProductPromise = null; // let a later call retry
          return {};
        });
    }
    return variantsByProductPromise;
  }

  // ---- reading the active filter/sort/search state -----------------------
  function emptyState() {
    return { options: {}, priceGte: null, priceLte: null, inStock: null, productTypes: [], sort: null, q: null, unsupported: [] };
  }

  function absorbParam(state, name, val) {
    if (name === 'sort_by') { state.sort = val; return; }
    if (name === 'q') { state.q = val; return; }
    if (name.indexOf('filter.') !== 0) return;
    if (name === 'filter.v.price.gte') { state.priceGte = parseFloat(val); return; }
    if (name === 'filter.v.price.lte') { state.priceLte = parseFloat(val); return; }
    if (name === 'filter.v.availability') { state.inStock = true; return; }
    var om = name.match(/^filter\.v\.option\.(.+)$/);
    if (om) { (state.options[om[1]] = state.options[om[1]] || []).push(val); return; }
    if (name === 'filter.p.product_type') { state.productTypes.push(val); return; }
    // filter.p.m.* (theme metafields) and anything else under filter.* —
    // no corresponding data in the module. Recorded, never sent, never
    // applied — see disableUnsupportedControls, which keeps these controls
    // from generating this in the first place.
    state.unsupported.push(name + '=' + val);
  }

  function readStateFromDom(root) {
    var state = emptyState();
    var els = root.querySelectorAll('[name]');
    for (var i = 0; i < els.length; i++) {
      var el = els[i];
      var name = el.getAttribute('name');
      if (!name) continue;
      var isCheckable = el.type === 'checkbox' || el.type === 'radio';
      if (isCheckable && !el.checked) continue;
      var val = el.value;
      if (val === '' || val == null) continue;
      absorbParam(state, name, val);
    }
    return state;
  }

  function readStateFromSearchParams(sp) {
    var state = emptyState();
    sp.forEach(function (val, name) { absorbParam(state, name, val); });
    return state;
  }

  // ---- building the API call(s) -------------------------------------
  function fetchFilteredProducts(state) {
    var optionNames = Object.keys(state.options);
    var primary = optionNames[0];
    var primaryValues = primary ? state.options[primary] : [null];
    var baseParams = {
      title: state.q || undefined,
      minPriceCents: state.priceGte != null ? Math.round(state.priceGte * 100) : undefined,
      maxPriceCents: state.priceLte != null ? Math.round(state.priceLte * 100) : undefined,
      inStock: state.inStock === true ? true : undefined
    };
    var calls = primaryValues.map(function (v) {
      var params = { title: baseParams.title, minPriceCents: baseParams.minPriceCents, maxPriceCents: baseParams.maxPriceCents, inStock: baseParams.inStock };
      if (primary) { params.optionName = primary; params.optionValue = v; }
      return apiGet('/product', params);
    });
    return Promise.all(calls).then(function (results) {
      var byId = {};
      results.forEach(function (list) { (list || []).forEach(function (p) { byId[p.id] = p; }); });
      var merged = Object.keys(byId).map(function (id) { return byId[id]; });

      var secondaryNames = optionNames.slice(1);
      if (!secondaryNames.length && !state.productTypes.length) return merged;

      return allVariantsByProduct().then(function (variantsByProduct) {
        return merged.filter(function (p) {
          if (state.productTypes.length && state.productTypes.indexOf(p.productType) === -1) return false;
          if (!secondaryNames.length) return true;
          var variants = variantsByProduct[p.id] || [];
          return secondaryNames.every(function (name) {
            var wanted = state.options[name];
            return variants.some(function (v) {
              return v.optionValues && wanted.indexOf(v.optionValues[name]) !== -1;
            });
          });
        });
      });
    });
  }

  // ---- client-side sort (no server support at all) -----------------------
  function minPriceOf(product, variantsByProduct) {
    var vs = variantsByProduct[product.id] || [];
    var min = null;
    vs.forEach(function (v) { if (min === null || v.priceCents < min) min = v.priceCents; });
    return min;
  }

  function applySort(products, sort, variantsByProduct) {
    if (!sort || !SUPPORTED_SORTS[sort]) return products;
    var arr = products.slice();
    arr.sort(function (a, b) {
      if (sort === 'title-ascending') return String(a.title).localeCompare(b.title);
      if (sort === 'title-descending') return String(b.title).localeCompare(a.title);
      if (sort === 'price-ascending') {
        var pa = minPriceOf(a, variantsByProduct), pb = minPriceOf(b, variantsByProduct);
        return (pa == null ? Infinity : pa) - (pb == null ? Infinity : pb);
      }
      if (sort === 'price-descending') {
        var pa2 = minPriceOf(a, variantsByProduct), pb2 = minPriceOf(b, variantsByProduct);
        return (pb2 == null ? -Infinity : pb2) - (pa2 == null ? -Infinity : pa2);
      }
      if (sort === 'created-ascending') return new Date(a.createdAt || 0) - new Date(b.createdAt || 0);
      if (sort === 'created-descending') return new Date(b.createdAt || 0) - new Date(a.createdAt || 0);
      return 0;
    });
    return arr;
  }

  // ---- grid detection + generic card templating --------------------------
  function findGrid() {
    var known = document.querySelector(
      '#product-grid, [id*="product-grid" i], product-list, .product-list, [class*="product-grid" i]'
    );
    if (known && known.children.length) return known;
    // Fallback: the element whose children are ALL "product card"-shaped
    // (each contains a link to /products/...) — works across themes that
    // don't use Dawn's naming conventions.
    var all = document.querySelectorAll('body *');
    var best = null, bestScore = 0;
    for (var i = 0; i < all.length; i++) {
      var el = all[i];
      var n = el.children.length;
      if (n < 2 || n > 200) continue;
      var score = 0;
      for (var j = 0; j < n; j++) {
        if (el.children[j].querySelector && el.children[j].querySelector('a[href*="/products/"]')) score++;
      }
      if (score === n && score > bestScore) { bestScore = score; best = el; }
    }
    return best;
  }

  function walkText(el, fn) {
    var tw = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    var node;
    while ((node = tw.nextNode())) fn(node);
  }

  function productHref(handle) {
    // Best effort only: the mirrored capture has a static page per
    // originally-captured product, so a product newly surfaced by a
    // filter may not resolve to a real local page — this is about the
    // GRID's contents, not per-product deep links. Known, reported gap.
    return 'products/' + handle + '.html';
  }

  var pristineCard = null;
  function populateCard(card, product, variants) {
    try {
      var priceCents = null, compareAtCents = null;
      (variants || []).forEach(function (v) {
        if (priceCents === null || v.priceCents < priceCents) priceCents = v.priceCents;
        if (v.compareAtPriceCents != null && (compareAtCents === null || v.compareAtPriceCents > compareAtCents)) {
          compareAtCents = v.compareAtPriceCents;
        }
      });
      var images = (product.images || []).slice().sort(function (a, b) { return (a.position || 0) - (b.position || 0); });
      var img = images.length ? images[0].src : null;

      var imgs = card.querySelectorAll('img');
      for (var i = 0; i < imgs.length; i++) {
        if (img) { imgs[i].setAttribute('src', img); imgs[i].removeAttribute('srcset'); imgs[i].removeAttribute('sizes'); }
        imgs[i].setAttribute('alt', product.title || '');
      }

      var links = card.querySelectorAll('a[href*="/products/"], a[href*="products/"]');
      for (var k = 0; k < links.length; k++) links[k].setAttribute('href', productHref(product.handle));

      var titleEl = card.querySelector('[class*="title" i], [class*="heading" i]');
      if (titleEl) titleEl.textContent = product.title;
      else if (links.length) links[0].textContent = product.title;

      if (priceCents != null) {
        var money = '$' + (priceCents / 100).toFixed(2);
        var priceEl = card.querySelector('[class*="price" i]');
        if (priceEl) {
          var replacedAny = false;
          walkText(priceEl, function (node) {
            if (/\$[\d,.]+/.test(node.nodeValue)) {
              node.nodeValue = node.nodeValue.replace(/\$[\d,.]+/, money);
              replacedAny = true;
            }
          });
          if (!replacedAny) priceEl.textContent = money;
        }
      }

      // No per-product review/rating data comes back from the module —
      // hide rather than show a stale rating that belongs to whatever
      // product the template card originally was.
      var ratings = card.querySelectorAll('[class*="rating" i]');
      for (var r = 0; r < ratings.length; r++) ratings[r].style.display = 'none';

      card.setAttribute('data-fl-product-id', product.id);
      if (card.hasAttribute('handle')) card.setAttribute('handle', product.handle);
      var idAttrEls = card.querySelectorAll('[data-product-id]');
      for (var d = 0; d < idAttrEls.length; d++) idAttrEls[d].setAttribute('data-product-id', product.id);
    } catch (e) {
      console.error('[forklaunch bridge] card populate failed for', product && product.title, e);
    }
  }

  function renderGrid(products, variantsByProduct) {
    var grid = findGrid();
    if (!grid) {
      console.error('[forklaunch bridge] could not locate a product grid on this page — filter/sort had no DOM to update');
      return false;
    }
    if (!pristineCard) {
      if (!grid.children.length) return false;
      pristineCard = grid.children[0].cloneNode(true);
    }
    if (!products.length) {
      grid.innerHTML = '';
      var empty = document.createElement('p');
      empty.setAttribute('data-fl-bridge', 'empty');
      empty.style.padding = '2rem 0';
      empty.textContent = 'No products match your filters.';
      grid.appendChild(empty);
      return true;
    }
    var frag = document.createDocumentFragment();
    products.forEach(function (p) {
      var card = pristineCard.cloneNode(true);
      populateCard(card, p, variantsByProduct[p.id] || []);
      frag.appendChild(card);
    });
    grid.innerHTML = '';
    grid.appendChild(frag);
    return true;
  }

  // ---- predictive search (theme's own dropdown, e.g. /search/suggest.json) --
  function predictiveSearch(rawUrl) {
    var u; try { u = new URL(rawUrl, location.href); } catch (e) { u = null; }
    var q = u ? u.searchParams.get('q') || '' : '';
    if (!q) return Promise.resolve({ resources: { results: { products: [] } } });
    return apiGet('/product', { title: q })
      .then(function (products) {
        return allVariantsByProduct().then(function (variantsByProduct) {
          var items = (products || []).slice(0, 10).map(function (p) {
            var price = minPriceOf(p, variantsByProduct);
            var images = (p.images || []);
            var img = images.length ? images[0].src : null;
            return {
              id: p.id,
              title: p.title,
              handle: p.handle,
              url: productHref(p.handle),
              featured_image: img ? { url: img, alt: p.title } : null,
              price: price != null ? (price / 100).toFixed(2) : null,
              price_min: price,
              price_max: price,
              available: (variantsByProduct[p.id] || []).length > 0
            };
          });
          return { resources: { results: { products: items } } };
        });
      })
      .catch(function (e) {
        console.error('[forklaunch bridge] predictive search failed', e);
        return { resources: { results: { products: [] } } };
      });
  }

  // Chain onto the fetch already patched above for cart handling — this
  // patch runs first and only claims /search/suggest*, everything else
  // (including plain /cart and the raw-forward branch for other /search
  // paths) falls through unchanged. XHR-based predictive search is a
  // known, undone gap — fetch() is what every theme checked for this
  // ticket actually uses.
  var _prevFetch = window.fetch;
  window.fetch = function (input, init) {
    try {
      var url = typeof input === 'string' ? input : (input && input.url) || '';
      var path = url.replace(/^https?:\/\/[^\/]+/, '').split('?')[0];
      if (/\/search\/suggest/.test(path)) {
        return predictiveSearch(url).then(jsonRes);
      }
    } catch (e) {}
    return _prevFetch.apply(window, arguments);
  };

  // ---- marking what genuinely cannot be backed as visibly inert ----------
  function disableUnsupportedControls(root) {
    var metafieldControls = root.querySelectorAll('[name^="filter.p.m."]');
    for (var i = 0; i < metafieldControls.length; i++) {
      var el = metafieldControls[i];
      el.disabled = true;
      el.title = 'Not available in this preview — no matching data in the migrated catalog.';
      if (el.id) {
        var label = root.querySelector('label[for="' + el.id + '"]');
        if (label) { label.style.opacity = '0.45'; label.style.cursor = 'not-allowed'; label.title = el.title; }
      }
    }
    var sortControls = root.querySelectorAll('[name="sort_by"]');
    for (var s = 0; s < sortControls.length; s++) {
      var el2 = sortControls[s];
      if (el2.tagName === 'SELECT') {
        var opts = el2.querySelectorAll('option');
        for (var o = 0; o < opts.length; o++) {
          if (opts[o].value && !SUPPORTED_SORTS[opts[o].value]) {
            opts[o].disabled = true;
            opts[o].textContent = opts[o].textContent.replace(/\s+$/, '') + ' (unavailable)';
          }
        }
      } else if (el2.value && !SUPPORTED_SORTS[el2.value]) {
        el2.disabled = true;
        if (el2.id) {
          var label2 = root.querySelector('label[for="' + el2.id + '"]');
          if (label2) { label2.style.opacity = '0.45'; label2.style.cursor = 'not-allowed'; label2.title = 'Not available in this preview.'; }
        }
      }
    }
  }

  // ---- wiring: intercept before the theme's own (broken, offline) AJAX --
  // Capture-phase + stopImmediatePropagation on document beats any
  // bubble-phase listener the theme attached to the control itself,
  // regardless of script load order, so the theme's own (offline-broken)
  // fetch to a Shopify section-rendering endpoint never fires.
  var renderToken = null;
  function runFilter(state) {
    var token = {};
    renderToken = token;
    return fetchFilteredProducts(state)
      .then(function (products) {
        return allVariantsByProduct().then(function (variantsByProduct) {
          if (renderToken !== token) return; // superseded by a later interaction
          var sorted = applySort(products, state.sort, variantsByProduct);
          renderGrid(sorted, variantsByProduct);
          if (state.unsupported.length) {
            console.warn('[forklaunch bridge] ignored filter(s) with no backing data in the module: ' + state.unsupported.join(', '));
          }
        });
      })
      .catch(function (e) {
        console.error('[forklaunch bridge] filter/sort/search request failed', e);
      });
  }

  var debounceTimer = null;
  function scheduleRun(state, immediate) {
    if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
    if (immediate) { runFilter(state); return; }
    debounceTimer = setTimeout(function () { runFilter(state); }, 200);
  }

  function isFilterControl(el) {
    if (!el || !el.getAttribute) return false;
    var name = el.getAttribute('name');
    if (!name) return false;
    return name === 'sort_by' || name === 'q' || name.indexOf('filter.') === 0;
  }

  function isFilterLink(el) {
    if (!el || el.tagName !== 'A') return false;
    var href = el.getAttribute('href');
    if (!href) return false;
    return /[?&](filter\.|sort_by=)/.test(href);
  }

  // Deliberately event-type-scoped rather than one handler for click+change+
  // input+submit alike: calling preventDefault() on a 'click' for a
  // checkbox/radio cancels its default action — the check-state toggle
  // itself — which silently made every checkbox filter permanently
  // unclickable during verification. 'click' is only ever meaningful here
  // for <a> filter links (to stop navigation); form controls are handled
  // on 'change'/'input', by which point the value has already updated.
  function handleInteraction(e) {
    if (e.type === 'click') {
      var link = e.target && e.target.closest ? e.target.closest('a') : null;
      if (link && isFilterLink(link)) {
        e.preventDefault(); e.stopImmediatePropagation();
        var url; try { url = new URL(link.href, location.href); } catch (err) { return; }
        scheduleRun(readStateFromSearchParams(url.searchParams), true);
      }
      return;
    }
    if (e.type === 'change' || e.type === 'input') {
      var target = e.target;
      if (!target || !isFilterControl(target)) return;
      if (target.getAttribute('name') === 'q') return; // handled by predictiveSearch via fetch, not a grid re-render
      // preventDefault() is a no-op on change/input (not cancelable) —
      // stopImmediatePropagation() is what matters: it keeps the theme's
      // own (offline-broken) facet-form listener from also firing.
      e.stopImmediatePropagation();
      scheduleRun(readStateFromDom(document), e.type !== 'input');
      return;
    }
    if (e.type === 'submit') {
      var form = e.target;
      if (form && form.tagName === 'FORM' && form.querySelector('[name^="filter."], [name="sort_by"]')) {
        e.preventDefault(); e.stopImmediatePropagation();
        scheduleRun(readStateFromDom(document), true);
      }
    }
  }

  try {
    disableUnsupportedControls(document);
  } catch (e) {
    console.error('[forklaunch bridge] failed to mark unsupported filters', e);
  }
  document.addEventListener('click', handleInteraction, true);
  document.addEventListener('change', handleInteraction, true);
  document.addEventListener('input', handleInteraction, true);
  document.addEventListener('submit', handleInteraction, true);
}

module.exports = { buildBridge };
