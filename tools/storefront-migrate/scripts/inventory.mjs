/**
 * inventory — enumerate what a storefront page ACTUALLY HAS, in a form that
 * can be compared between the live site and the clone.
 *
 * Every gate before this one asked a yes/no question the author already knew
 * to ask: is the shim running, is the page under 12MB, did anything 404. They
 * are good gates and they all pass on a clone that is missing half the live
 * site's sections, because nobody wrote an assertion for the section that went
 * missing. You cannot enumerate defects you have not met yet.
 *
 * So this inverts it. The live site is the oracle: whatever it renders is the
 * specification, and the clone is measured against that specification rather
 * than against a list of remembered bugs.
 *
 * Two rules keep it honest, and both were learned from real storefronts:
 *
 *   1. FEATURES, NOT VALUES. Never "the price is $23" — live stock counts,
 *      rotating banners, "14 people are viewing this", A/B buckets and
 *      personalisation legitimately differ between two loads of the SAME live
 *      page. Assert that a thing EXISTS and RESPONDS. A price element must be
 *      there; what it says is not our business.
 *
 *   2. ONLY WHAT IS STABLE LIVE COUNTS. The live page is sampled twice and
 *      only what appears in BOTH samples becomes a requirement. One measured
 *      store changed 38% of its elements between two loads five seconds apart;
 *      without this, a third of every report would be noise, and a report that
 *      is a third noise gets ignored entirely — which is worse than no report.
 *
 * What gets enumerated, and why each one is the right shape:
 *
 *   headings      the skeleton of "sections". Text-keyed, so it survives
 *                 completely different markup between a Liquid theme and its
 *                 captured DOM. A missing heading is a missing section.
 *   landmarks     header/nav/main/footer by role — the page's chrome.
 *   fonts         document.fonts, by family. THE fidelity signal: fall back
 *                 from the brand typeface and every text metric changes, so
 *                 headers that fit at the real width start colliding. It reads
 *                 as a broken site, not a restyled one.
 *   media         images and videos that actually painted (naturalWidth > 0),
 *                 plus the ones that did not — a broken image is a defect
 *                 whatever the live page has.
 *   controls      every interactive element by ROLE + ACCESSIBLE NAME, never
 *                 by CSS selector. Selectors are per-theme; "a button whose
 *                 accessible name is Checkout" is per-web.
 *   navTargets    where the page's own links point, so the clone can be asked
 *                 whether it actually serves them.
 *
 * Nothing here is Shopify-specific. It works on a Liquid theme, a Hydrogen
 * storefront and a hand-written page alike, because it only asks the questions
 * the accessibility tree can answer.
 */

/**
 * Runs INSIDE the page. Self-contained by necessity — it is serialised across
 * the CDP boundary, so it may not close over anything from this module.
 */
export const SNAPSHOT = function () {
  const norm = (s) => String(s || '').replace(/\s+/g, ' ').trim().slice(0, 120);

  /**
   * The key a live feature is compared by — NOT what is shown in the report.
   *
   * A clone is a frozen snapshot; the live site keeps moving. Between the
   * capture and the check, "4.9 based on 4540 Reviews" becomes "4.9 based on
   * 4561 Reviews", "Total 3 star reviews: 32" becomes ": 33", a price changes,
   * a cart badge reads Cart(1) instead of Cart(0). Compared literally, every
   * one of those is a missing feature — and they are not missing, they are
   * values, which this gate has no business asserting.
   *
   * Sampling live twice cannot catch these: they are stable across two loads
   * five seconds apart and different from the capture taken last month. So
   * every run of digits collapses to a single `#`. "Total 3 star reviews: 32"
   * and "Total 5 star reviews: 4.3k" both become "total # star reviews: #" —
   * the same FEATURE, which is exactly the level this gate operates at. The
   * unnormalised name is kept alongside for the report, so a human still reads
   * something recognisable.
   */
  const featureKey = (s) => norm(s).toLowerCase()
    .replace(/[0-9][0-9.,]*\s*[km%]?/g, '#')
    .replace(/[$£€¥]\s*#/g, '#')
    // Sale phrasing is a VALUE that changes between visits ("Original price:
    // $59.99", "20% off" appear when a promotion is on), not a feature.
    .replace(/\b(original|regular|sale|compare at) price:?/g, ' ')
    .replace(/#\s*off\b/g, ' ')
    .replace(/#(\s*#)+/g, '#')
    .replace(/\s+/g, ' ')
    .trim();
  // An internal link is identified by its DESTINATION. Its accessible name
  // is whatever sits inside the card — a vendor's star rating, a sale badge,
  // a price — all of which differ between the live site and an offline clone
  // without the link being any less present. Same destination, same feature.
  const hrefKey = (el) => {
    try {
      if (!el.getAttribute) return null;
      // The clone rewrites a link to an uncaptured page (search, cart,
      // account) to "#" and keeps the original path in data-mirror-uncaptured;
      // a link it resolved locally keeps its target in data-fl-href. Either is
      // the destination the live site has.
      const h = el.getAttribute('data-mirror-uncaptured') || el.getAttribute('data-fl-href') || el.getAttribute('href');
      if (!h || /^(#|mailto:|tel:|javascript:|data:)/i.test(h)) return null;
      const u = new URL(h, location.href);
      if (u.host && u.host.replace(/^www\./, '') !== location.host.replace(/^www\./, '')) return null;
      let p = u.pathname.replace(/\.html$/, '').replace(/\/+$/, '').replace(/^\/+/, '') || '/';
      if (p === 'index') p = '/';                       // the clone's homepage file
      // Canonical form on BOTH sides, mirroring urlmap.js: a product under a
      // collection is the product; a deeper path folds to a/b-c, which is the
      // file the clone stores it as, so its href already reads that way.
      const cp = p.match(/^collections\/[^/]+\/products\/([^/]+)$/);
      if (cp) p = 'products/' + cp[1];
      const segs = p.split('/');
      if (segs.length === 3 && !/^(products|collections|pages)$/.test(segs[0])) p = segs[0] + '/' + segs[1] + '-' + segs[2];
      return 'href:' + p.toLowerCase();
    } catch (_) { return null; }
  };

  const visible = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return false;
    const c = getComputedStyle(el);
    return c.visibility !== 'hidden' && c.display !== 'none' && Number(c.opacity) > 0.01;
  };

  /**
   * Accessible name, computed the way a screen reader would in the cases that
   * matter on a storefront. Deliberately not the full accname spec: the point
   * is a key that is identical on the live page and on the capture, and
   * aria-label / alt text / visible label covers every control a shopper
   * touches. An icon-only button with no name at all is keyed by its class
   * signature instead, so it is still comparable rather than dropped.
   */
  const accName = (el) => {
    const aria = el.getAttribute('aria-label');
    if (aria && aria.trim()) return norm(aria);
    const by = el.getAttribute('aria-labelledby');
    if (by) {
      const t = by.split(/\s+/).map((id) => document.getElementById(id)?.textContent || '').join(' ');
      if (t.trim()) return norm(t);
    }
    const own = norm(el.textContent);
    if (own) return own;
    const img = el.querySelector('img[alt]');
    if (img && img.getAttribute('alt').trim()) return norm(img.getAttribute('alt'));
    const svgTitle = el.querySelector('svg title, svg > desc');
    if (svgTitle && svgTitle.textContent.trim()) return norm(svgTitle.textContent);
    const t = el.getAttribute('title') || el.getAttribute('placeholder') || el.getAttribute('name') || el.value;
    if (t && String(t).trim()) return norm(t);
    // Last resort: something stable and shape-describing. A theme's own class
    // names survive capture untouched, so this still matches across sides.
    const cls = norm((el.className && el.className.baseVal !== undefined ? el.className.baseVal : el.className) || '')
      .split(' ').filter(Boolean).slice(0, 2).join('.');
    return cls ? '·' + cls : '·' + el.tagName.toLowerCase();
  };

  const role = (el) => {
    const r = el.getAttribute('role');
    if (r) return r.toLowerCase();
    const tag = el.tagName.toLowerCase();
    if (tag === 'a') return el.hasAttribute('href') ? 'link' : 'generic';
    if (tag === 'button') return 'button';
    if (tag === 'summary') return 'button';
    if (tag === 'select') return 'combobox';
    if (tag === 'textarea') return 'textbox';
    if (tag === 'input') {
      const t = (el.type || 'text').toLowerCase();
      if (t === 'submit' || t === 'button' || t === 'reset') return 'button';
      if (t === 'checkbox') return 'checkbox';
      if (t === 'radio') return 'radio';
      if (t === 'search') return 'searchbox';
      if (t === 'hidden') return 'none';
      return 'textbox';
    }
    if (tag === 'label') return 'label';
    return 'generic';
  };

  /**
   * What KIND of storefront control this is, by name and by contract.
   *
   * The kinds exist so a repair (and a human reading the report) knows which
   * feature broke, not merely that "a button is missing". Order matters: the
   * first match wins, and the more specific patterns come first — "add to
   * cart" must not be classified as a cart control.
   *
   * `/cart/add` is the one true CONTRACT here (every Shopify theme posts to
   * it, headless ones included), so a form action is trusted over any name.
   * Everything else is a convention, which is exactly why it is matched by
   * accessible name across many phrasings rather than by one selector.
   */
  const classify = (el, name, r) => {
    const n = name.toLowerCase();
    const href = (el.getAttribute('href') || '').toLowerCase();
    const form = el.closest('form');
    const action = ((form && form.getAttribute('action')) || '').toLowerCase();

    if (/\/cart\/add/.test(action) || /\/cart\/add/.test(href)) return 'addcart';
    if (/^(add to (cart|bag|basket|order)|add \+|add$|add to my (cart|bag)|subscribe (and|&) save)/.test(n)) return 'addcart';
    // "Buy it now" is Shopify's DYNAMIC CHECKOUT button: it does not add to a
    // cart, it jumps straight to the hosted checkout. Filed under addcart it
    // was excluded from the bridge's checkout interception, and clicking it on
    // a demo walked the viewer out of the clone and onto the merchant's real
    // Shopify checkout — the exact leak the interception exists to prevent.
    if (/^(checkout|check out|proceed to checkout|continue to checkout|place (the )?order|pay now|complete (my )?order|secure checkout|go to checkout|buy now|buy it now)/.test(n)) return 'checkout';
    if (/\/checkout/.test(href)) return 'checkout';
    if (/^(cart|bag|basket|shopping (cart|bag|basket)|view (cart|bag)|open (cart|bag)|your (cart|bag)|my (cart|bag))\b/.test(n)) return 'cart';
    if (href === '/cart' || /\/cart(\?|$)/.test(href)) return 'cart';
    if (r === 'searchbox' || /^(search|open search|search (the )?(store|site|products))/.test(n)) return 'search';
    if (/^(menu|open menu|main menu|navigation|toggle (the )?menu|hamburger)/.test(n)) return 'menu';
    if (/^(next|previous|prev|next slide|previous slide|go to slide|slide \d)/.test(n)) return 'carousel';
    if (/^(filter|filters|sort|sort by|refine|show \d+ (results|products))/.test(n)) return 'filter';
    if (/^(accept|allow all|got it|subscribe|sign up|close|dismiss|×|✕)$/.test(n)) return 'dismiss';
    return r === 'link' ? 'link' : 'control';
  };

  /**
   * Which third-party app, if any, rendered this element.
   *
   * Reviews, loyalty balances and subscription widgets live in other vendors'
   * databases — Okendo, Yotpo, Judge.me, Stamped, Loox, Junip, Rebuy, Klaviyo
   * — not in Shopify. They cannot be migrated by looking at a page, and the
   * offline guarantee deliberately blocks their scripts so a demo cannot
   * beacon from a client's machine. So a control they own is a POLICY
   * outcome, not a defect, and the report has to be able to say which vendor
   * and why rather than listing eleven anonymous missing buttons.
   *
   * Matched on the vendor's own class/id prefix, which is how each of these
   * mounts itself. It is a list, and lists go stale — but naming a vendor is a
   * claim that needs evidence, and a wrong guess here would excuse a real
   * defect. Anything unmatched stays a defect, which is the safe direction.
   */
  const VENDOR_SIG = [
    ['okendo', /(^|[\s_-])oke-|okendo/i], ['yotpo', /yotpo/i], ['judge.me', /jdgm/i],
    ['stamped', /stamped-/i], ['loox', /(^|[\s_-])loox/i], ['junip', /junip/i],
    ['reviews.io', /ruk_|reviewsio/i], ['rebuy', /rebuy/i], ['klaviyo', /klaviyo|kl-private/i],
    ['smile.io', /smile-ui|smile_/i], ['recharge', /recharge|rc_widget/i],
    ['gorgias', /gorgias/i], ['attentive', /attentive/i], ['shopify-reviews', /spr-|shopify-product-reviews/i],
    // Cookie-consent banners. Shopify's own widget mounts as
    // polaris-consent-widget / _consent_app-…; the common apps carry their
    // names. The banner's markup is captured but its script is blocked, so it
    // never shows on the clone — a POLICY outcome, and one every EU-facing
    // store will hit ("we take privacy seriously" on gorillamind.com).
    ['cookie-consent', /polaris-consent|consent[_-]app|cookie[-_]?consent|consentmo|cookiebot|onetrust|pandectes|cc-window|cookie[-_]?banner/i],
    // Store locators render from a maps vendor at runtime (liquiddeath.com's
    // /pages/where-to-buy: "find your product", product-type selects, "Open
    // this area in Google Maps"). Matched on the locator apps' own prefixes
    // and Google Maps' container classes.
    ['store-locator', /storepoint|storemapper|stockist|store-?locator|storelocator|gm-style|pac-container|maps-widget|closeby|storerocket/i]
  ];
  const vendorOf = (el) => {
    let n = el, depth = 0;
    while (n && n.nodeType === 1 && depth++ < 8) {
      const sig = ((n.id || '') + ' ' + ((n.className && n.className.baseVal !== undefined ? n.className.baseVal : n.className) || '')) + ' ' +
        (n.getAttribute('data-oke-container') !== null ? 'okendo' : '');
      for (const [name, re] of VENDOR_SIG) if (re.test(sig)) return name;
      n = n.parentElement;
    }
    return null;
  };

  // Content a recommender renders is a VALUE, not a feature: which products
  // "You may also like" shows, their titles, flavor selects and add buttons
  // differ from visit to visit and certainly from live to an offline clone.
  // The feature is that the block exists and is filled. Anything inside such
  // a container is tagged with the container so the gate compares presence,
  // not names. Matched on the container's own id/class/tag — evidence.
  const DYNAMIC_SIG = /recommend|upsell|also-like|complete-the-look|complete-your|frequently-bought|related-products|cross-sell|you-may|bundle-and-save|recently-viewed|cart-drawer|drawer-cart|mini-cart|cart__drawer|cart-notification/i;
  const dynamicOf = (el) => {
    let n = el, d = 0;
    while (n && n.nodeType === 1 && d++ < 12) {
      const sig = n.tagName + ' ' + (n.id || '') + ' ' + (typeof n.className === 'string' ? n.className : '');
      if (DYNAMIC_SIG.test(sig)) return (n.id || n.tagName.toLowerCase()).slice(0, 60);
      n = n.parentElement;
    }
    return null;
  };

  // ---- headings: the section skeleton ------------------------------------
  const headings = [];
  const headingVendor = {};
  const headingDynamic = {};
  // A heading inside a carousel slide is visible only while its slide is
  // active. Live and clone sit on different slides at different moments, so
  // slide content is judged by "the carousel has content", never slide by
  // slide. Matched on the slide libraries' own class names — evidence.
  const SLIDE_SIG = /slide|swiper-slide|slick-slide|flickity-cell|splide__slide|glide__slide|carousel__slide|carousel-item/i;
  const inSlide = (el) => { let n = el, d = 0; while (n && n.nodeType === 1 && d++ < 8) { if (SLIDE_SIG.test(typeof n.className === 'string' ? n.className : '')) return true; n = n.parentElement; } return false; };
  const headingSlide = {};
  for (const h of document.querySelectorAll('h1,h2,h3,h4,[role="heading"]')) {
    if (!visible(h)) continue;
    const t = norm(h.textContent);
    if (t.length < 2) continue;
    const k = featureKey(t);
    headings.push(k);
    const v = vendorOf(h);
    if (v) headingVendor[k] = v;
    const dy = dynamicOf(h);
    if (dy) headingDynamic[k] = dy;
    if (inSlide(h)) headingSlide[k] = true;
  }

  // ---- landmarks ---------------------------------------------------------
  const landmarks = [];
  for (const [sel, nameOf] of [['header,[role=banner]', 'banner'], ['nav,[role=navigation]', 'navigation'],
    ['main,[role=main]', 'main'], ['footer,[role=contentinfo]', 'contentinfo'],
    ['form[role=search],[role=search]', 'search']]) {
    // A landmark inside a vendor widget (Yotpo's review-search box is a
    // role=search) belongs to the vendor, not to the page's chrome.
    if ([...document.querySelectorAll(sel)].some((el) => visible(el) && !vendorOf(el))) landmarks.push(nameOf);
  }

  // ---- media -------------------------------------------------------------
  const imgs = [...document.images];
  // An <img> that finished loading with zero intrinsic width is a dead asset
  // the page still has a hole for. Lazy images that have not started are not
  // counted — they are not broken, they are not needed yet.
  //
  // Split by ORIGIN, and that split is the whole assertion. Every single
  // "broken image" on graza.co's clone turned out to be a 1×1 tracking pixel:
  // bidr.io, roeye.com, dstillery.com. They are <img> tags because that is how
  // ad networks beacon, they have no pixels because we deliberately refuse
  // them, and they are the offline guarantee working exactly as designed.
  // Counting them as defects put five permanent red marks on every page and
  // pointed a repair at a problem that did not exist.
  const brokenImgs = imgs.filter((i) => i.complete && i.naturalWidth === 0 && (i.currentSrc || i.src));
  const isLocalSrc = (u) => { try { return /^(localhost|127\.0\.0\.1)/.test(new URL(u, location.href).host); } catch { return false; } };
  const media = {
    imagesRendered: imgs.filter((i) => i.complete && i.naturalWidth > 0).length,
    imagesBroken: brokenImgs.filter((i) => isLocalSrc(i.currentSrc || i.src)).length,
    imagesBrokenExternal: brokenImgs.length - brokenImgs.filter((i) => isLocalSrc(i.currentSrc || i.src)).length,
    brokenSamples: brokenImgs.filter((i) => isLocalSrc(i.currentSrc || i.src))
      .slice(0, 4).map((i) => norm(i.currentSrc || i.src).slice(-70)),
    videos: document.querySelectorAll('video').length,
    // Background images are how storefronts ship most of their art, and they
    // never appear in document.images. Counting them separately stops a clone
    // that lost every hero from scoring the same as one that lost none.
    // Vendor widgets' own art (Yotpo's 13 review borders on gorillamind.com)
    // is not the storefront's art; counted out so the floor is the theme's.
    backgroundImages: [...document.querySelectorAll('*')].filter((el) => {
      const b = getComputedStyle(el).backgroundImage;
      return b && b !== 'none' && b.includes('url(') && !vendorOf(el);
    }).length
  };

  // ---- fonts -------------------------------------------------------------
  // document.fonts holds the @font-face rules the page declared; status tells
  // us which actually loaded. A declared-but-unloaded face is a fallback in
  // disguise, which is why both numbers are kept.
  const fontFaces = [];
  try {
    document.fonts.forEach((f) => fontFaces.push({ family: norm(f.family).replace(/^["']|["']$/g, '').toLowerCase(), status: f.status }));
  } catch (_) {}
  const fontsLoaded = [...new Set(fontFaces.filter((f) => f.status === 'loaded').map((f) => f.family))];
  const fontsDeclared = [...new Set(fontFaces.map((f) => f.family))];
  // What the page is actually PAINTING with — a declared face nothing uses is
  // not a feature, and a body rendering in Helvetica when it should be in the
  // brand's face is the defect that matters.
  const fontsInUse = [...new Set([...document.querySelectorAll('h1,h2,h3,p,a,button,li,span')]
    .filter(visible).slice(0, 400)
    .map((el) => norm(getComputedStyle(el).fontFamily.split(',')[0]).replace(/^["']|["']$/g, '').toLowerCase())
    .filter(Boolean))];

  // ---- controls ----------------------------------------------------------
  const CANDIDATE = 'a[href],button,summary,select,input,textarea,[role=button],[role=link],' +
    '[role=tab],[role=checkbox],[role=radio],[role=combobox],[onclick]';
  const controls = [];
  const seen = new Set();
  // Stale stamps are cleared and new ids never reuse an old number. Both
  // matter: a navigation wipes the attributes, and re-stamping from 1 would
  // hand id 45 to a completely different element — so a probe looking up a
  // remembered id would silently actuate the wrong control and report whatever
  // that one did. Ids that only ever go up make a stale lookup a clean miss.
  for (const old of document.querySelectorAll('[data-fl-ctl]')) old.removeAttribute('data-fl-ctl');
  window.__flStampBase = (window.__flStampBase || 0) + 100000;
  let stamp = window.__flStampBase;
  for (const el of document.querySelectorAll(CANDIDATE)) {
    const r = role(el);
    if (r === 'none') continue;
    // Off-screen controls still count: a cart drawer's Checkout button and a
    // mobile menu's links are hidden until opened, and they are exactly the
    // features most likely to be lost in a capture. Only zero-sized elements
    // with no parent at all are dropped.
    const inDom = el.isConnected;
    if (!inDom) continue;
    const name = accName(el);
    const kind = classify(el, name, r);
    const hk = r === 'link' ? hrefKey(el) : null;
    const key = r + ' ' + (hk || featureKey(name));
    if (seen.has(key)) continue;
    seen.add(key);
    // Stamped so the control can be actuated later without re-deriving a
    // selector for it. Re-finding a control by CSS path is exactly what makes
    // a tool like this theme-specific; an attribute we put there ourselves is
    // theme-proof, and an added data- attribute changes nothing about how the
    // page renders or behaves.
    const id = ++stamp;
    try { el.setAttribute('data-fl-ctl', String(id)); } catch (_) {}
    controls.push({
      ctl: id, role: r, name, key, kind, vendor: vendorOf(el), dynamic: dynamicOf(el),
      visible: visible(el),
      href: el.getAttribute && el.getAttribute('href') || null
    });
  }

  // ---- variant / option pickers ------------------------------------------
  // Discovered structurally rather than by class name: a group of two or more
  // radios sharing a name, or a <select> with options, sitting in the same
  // form or card as an add-to-cart control. That description holds on a Dawn
  // theme, on a headless React PDP, and on a bespoke one.
  const optionGroups = [];
  const addForms = [...document.querySelectorAll('form')].filter((f) => /\/cart\/add/.test(f.getAttribute('action') || ''));
  // Variant pickers live in add-to-cart forms. Without one on the page there
  // are none — a collection page's filter facets (colour, size, "bundle
  // savings") are filters, asserted separately, not pickers. Scanning the
  // whole body on such pages reported brooklinen's facets as 8 missing pickers.
  const scopes = addForms;
  // A picker inside the cart drawer (an upsell's variant select) exists only
  // once the cart has an item. Recorded so the gate can treat it as a
  // cart-state feature: provable with a module, not in browse-only mode.
  const inCartDrawer = (el) => {
    let n = el, d = 0;
    while (n && n.nodeType === 1 && d++ < 14) {
      const sig = (n.tagName + ' ' + (n.id || '') + ' ' + (typeof n.className === 'string' ? n.className : '')).toLowerCase();
      if (/cart-drawer|drawer-cart|cart-block|mini-cart|minicart|cart__drawer|cartdrawer|cart-upsell|drawer__cart/.test(sig)) return true;
      n = n.parentElement;
    }
    return false;
  };
  for (const scope of scopes) {
    const inCart = inCartDrawer(scope);
    const dyn = dynamicOf(scope);
    const byName = new Map();
    for (const inp of scope.querySelectorAll('input[type=radio],input[type=checkbox]')) {
      const n = inp.name || accName(inp);
      byName.set(n, (byName.get(n) || 0) + 1);
    }
    for (const [n, count] of byName) if (count >= 2) optionGroups.push({ kind: 'radios', name: norm(n).toLowerCase(), count, inCart, dynamic: dyn });
    for (const sel of scope.querySelectorAll('select')) {
      if (sel.options && sel.options.length >= 2) optionGroups.push({ kind: 'select', name: accName(sel).toLowerCase(), count: sel.options.length, inCart: inCart || inCartDrawer(sel), dynamic: dyn || dynamicOf(sel) });
    }
    // Swatch/size buttons: several sibling buttons or labels with short text
    // inside one container is the universal shape of a variant picker.
    for (const box of scope.querySelectorAll('fieldset,[role=radiogroup],ul,div')) {
      const kids = [...box.children].filter((c) => /^(button|label)$/i.test(c.tagName) ||
        c.getAttribute('role') === 'radio' || c.querySelector('input[type=radio]'));
      if (kids.length >= 2 && kids.length <= 12 &&
          kids.every((k) => norm(k.textContent).length > 0 && norm(k.textContent).length <= 24)) {
        optionGroups.push({ kind: 'swatches', name: norm(box.getAttribute('aria-label') || box.getAttribute('data-option') || kids.map((k) => norm(k.textContent)).join('|')).toLowerCase(), count: kids.length });
      }
    }
  }

  // ---- where the page's own links point ----------------------------------
  const navTargets = [...new Set([...document.querySelectorAll('a[href]')]
    .map((a) => a.getAttribute('href'))
    .filter((h) => h && !/^(mailto:|tel:|javascript:|#|data:)/i.test(h)))];

  return {
    title: norm(document.title),
    headings: [...new Set(headings)], headingVendor, headingDynamic, headingSlide,
    // Which theme template rendered this page. Shopify stamps every section
    // with it; a store running an A/B test (or switching templates mid-day,
    // as gorillamind.com did) serves a different one than the capture, and
    // then a section-by-section comparison is meaningless, not failing.
    templateId: (() => { try { const s = document.querySelector('[id^="shopify-section-template--"]'); return s ? s.id.replace('shopify-section-', '').split('__')[0] : null; } catch (_) { return null; } })(),
    landmarks,
    media,
    fontsLoaded, fontsDeclared, fontsInUse,
    controls,
    optionGroups: [...new Map(optionGroups.map((g) => [g.kind + ':' + g.name, g])).values()],
    navTargets,
    // Cheap structural size. Used only to catch a clone that rendered a blank
    // or error page — never compared for equality.
    domNodes: document.querySelectorAll('*').length,
    textLength: (document.body.innerText || '').length,
    scrollHeight: document.documentElement.scrollHeight
  };
};

/**
 * Install a response detector. Read with `readResponse` after actuating a
 * control.
 *
 * Detecting "did that button do anything" by looking for a DOM change alone
 * does not work on a storefront: hero videos, marquees and count-up animations
 * mutate the tree continuously, so EVERY click looks like it worked. The fix
 * is a baseline — measure the page's idle mutation rate first, and only count
 * a burst well above it. The other four signals (URL, aria-expanded, a newly
 * visible overlay, a network request) are unambiguous on their own.
 */
export const INSTALL_PROBE = function () {
  window.__flProbe = { mut: 0, req: 0, start: Date.now() };
  const mo = new MutationObserver((recs) => { window.__flProbe.mut += recs.length; });
  mo.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
  window.__flProbe.stop = () => mo.disconnect();
  const of = window.fetch;
  window.fetch = function (...a) { window.__flProbe.req++; return of.apply(this, a); };
  const ox = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (...a) { window.__flProbe.req++; return ox.apply(this, a); };
  const overlays = () => [...document.querySelectorAll('*')].filter((el) => {
    const c = getComputedStyle(el);
    if (c.position !== 'fixed' && c.position !== 'absolute') return false;
    if (c.visibility === 'hidden' || c.display === 'none' || Number(c.opacity) < 0.05) return false;
    const r = el.getBoundingClientRect();
    return r.width > 120 && r.height > 120;
  }).length;
  window.__flProbe.overlays = overlays;
  window.__flProbe.expanded = () => [...document.querySelectorAll('[aria-expanded],[open],[aria-selected],dialog')]
    .map((e) => (e.getAttribute('aria-expanded') || '') + (e.hasAttribute('open') ? '1' : '0') + (e.getAttribute('aria-selected') || '')).join('');
  window.__flProbe.snap = () => ({
    mut: window.__flProbe.mut, req: window.__flProbe.req,
    overlays: overlays(), expanded: window.__flProbe.expanded(), url: location.href
  });
};

/**
 * Did actuating this control cause the page to respond?
 *
 * `baseline` is a snapshot pair taken over the same wait WITHOUT clicking, so
 * the mutation threshold adapts to whatever this particular page does while
 * idle. Everything else is compared as a change, not as a level.
 */
export function responded(before, after, baselineMutations) {
  if (after.url !== before.url) return { yes: true, why: 'navigated' };
  if (after.expanded !== before.expanded) return { yes: true, why: 'aria/open state changed' };
  if (after.overlays > before.overlays) return { yes: true, why: 'overlay appeared' };
  if (after.req > before.req) return { yes: true, why: `${after.req - before.req} request(s)` };
  const burst = after.mut - before.mut;
  const floor = Math.max(8, baselineMutations * 3);
  if (burst > floor) return { yes: true, why: `${burst} DOM mutations (idle ${baselineMutations})` };
  return { yes: false, why: `no response (${burst} mutations, idle ${baselineMutations})` };
}

/**
 * Take one inventory of one URL, with the page's console errors and failed
 * requests recorded alongside.
 *
 * Errors and dead requests are gathered per-navigation rather than per-run:
 * charging a product page for the homepage's failures makes a report nobody
 * can act on.
 */
export async function inventoryRoute(page, url, { settle = 4000, scroll = true } = {}) {
  const consoleErrors = [];
  const deadRequests = [];
  // The message text alone cannot be classified: Chrome renders a blocked
  // third-party script as the bare string "Failed to load resource: the server
  // responded with a status of 404", with the URL only in the location. That
  // is the difference between "the offline guarantee is working" and "we lost
  // a file", so the location is kept.
  const onConsole = (m) => {
    if (m.type() !== 'error') return;
    let where = '', line = 0;
    try { where = m.location()?.url || ''; line = m.location()?.lineNumber || 0; } catch (_) {}
    consoleErrors.push({ text: m.text().slice(0, 200), url: where.slice(0, 200), line });
  };
  // Keep WHERE the error came from, not just what it said. The classifier's
  // strongest test is origin — an error thrown by a vendor's script is that
  // vendor's problem, not the capture's — and it can only run if the frame URL
  // survives. Dropping it here forced every uncaught error through text
  // heuristics, which cannot name a vendor that ships from an anonymous CDN host
  // (Loomi via CloudFront) or throws a message with no identifier in it ("Token
  // is not a valid GUID", a null .append). Those were the 14 errors the gate
  // mis-reported as capture defects on graza.co.
  const onPageError = (e) => {
    const stack = String(e.stack || '');
    const frame = stack.match(/\((https?:\/\/[^)\s]+?)(?::(\d+))?(?::\d+)?\)/) || stack.match(/at (https?:\/\/[^\s:]+?)(?::(\d+))?(?::\d+)?(?:\s|$)/);
    // Keep the LINE too: an error thrown from an inline script has the page
    // as its frame, and only the line can say which inline script it was.
    consoleErrors.push({ text: 'uncaught: ' + String(e.message).slice(0, 200), url: frame ? frame[1] : '', line: frame && frame[2] ? Number(frame[2]) : 0 });
  };
  // Host and reason are both kept. Without them a deliberate offline block —
  // a tracker refused by the served CSP, which is the guarantee working — is
  // indistinguishable from an asset the capture failed to save, and the report
  // fills with two dozen "failures" that are the feature behaving correctly.
  const rec = (u, status, why) => {
    let url; try { url = new URL(u); } catch { return; }
    deadRequests.push({
      host: url.host, path: url.pathname.slice(0, 100), status, why,
      ours: /^localhost|^127\.0\.0\.1/.test(url.host)
    });
  };
  const onResponse = (r) => { if (r.status() >= 400) rec(r.url(), r.status(), 'http'); };
  const onFailed = (r) => {
    // A request the browser aborted is not a dead asset — media range requests
    // and prefetches are cancelled constantly and mean nothing.
    const f = r.failure();
    if (f && !/ERR_ABORTED/.test(f.errorText)) rec(r.url(), 0, f.errorText.replace('net::', ''));
  };
  // Every third-party host the page ran a SCRIPT from. Recorded on the live
  // side, this becomes evidence: an error on the clone naming an identifier
  // that matches one of these hosts (`window.loomi.conf` against loomi.ai) is
  // attributable to a vendor script we deliberately did not ship, rather than
  // to anything the capture broke. It replaces a hand-maintained list of
  // vendor globals with something derived from the page itself, so it stays
  // correct for a store using a vendor nobody has heard of.
  const scriptHosts = new Set();
  // Every third-party host the page contacted AT ALL (fetch, XHR, beacon).
  // A vendor injected inline by a Shopify app (Loomi on graza.co) never loads
  // a <script> from its own host, but it does talk to it — sdk.loomi-stg.xyz —
  // and that is the evidence the identifier match in check-features needs.
  const contactedHosts = new Set();
  const onRequest = (r) => {
    try {
      const h = new URL(r.url()).host;
      if (!h || /^localhost|^127\.0\.0\.1/.test(h)) return;
      contactedHosts.add(h);
      if (r.resourceType() === 'script') scriptHosts.add(h);
    } catch (_) {}
  };
  page.on('console', onConsole);
  page.on('pageerror', onPageError);
  page.on('response', onResponse);
  page.on('requestfailed', onFailed);
  page.on('request', onRequest);
  // An unhandled rejection whose reason is not an Error reaches pageerror as
  // its TYPE NAME — "Array(1)", "Object" — which names nothing. Keep the
  // reason's contents (Shopify's feature loader rejects with
  // [Error("load error: https://…/consent-tracking-api.js")]) so the
  // classifier can see the external host the message carries.
  await page.addInitScript(() => {
    window.__flRej = [];
    window.addEventListener('unhandledrejection', (e) => {
      const r = e.reason;
      if (r instanceof Error) return;
      const one = (x) => (x && typeof x === 'object') ? String(x.message || (() => { try { return JSON.stringify(x); } catch { return String(x); } })()) : String(x);
      const parts = Array.isArray(r) ? r.slice(0, 3).map(one) : [one(r)];
      window.__flRej.push(parts.join(' | ').slice(0, 220));
    });
  }).catch(() => {});
  try {
    await page.goto(url, { waitUntil: 'load', timeout: 60000 });
    await page.waitForTimeout(settle);
    if (scroll) {
      // Storefronts lazy-load nearly everything below the fold. Without this
      // the inventory only ever sees the hero, and every section beneath it
      // reads as "present on both sides" because neither side rendered it.
      await page.evaluate(async () => {
        for (let y = 0; y < document.body.scrollHeight; y += window.innerHeight) {
          window.scrollTo(0, y);
          await new Promise((r) => setTimeout(r, 220));
        }
        window.scrollTo(0, 0);
      }).catch(() => {});
      await page.waitForTimeout(1800);
    }
    // "imagery renders" asks whether the images CAN load, not when. The clone
    // server marks every image loading="lazy" to keep first paint cheap
    // (heroserve-fl.ts), the crawl strips it, and live themes mix both, so
    // the same page counted three different ways depending on which side and
    // which scroll pass. Kettleandfire's clone read 37 rendered vs 98 live
    // with every file served fine. Equalise: flip lazy to eager on whichever
    // side we are measuring and wait for the loads (capped) before counting.
    await page.evaluate(async () => {
      const imgs = [...document.images].filter((i) => !(i.complete && i.naturalWidth > 0));
      for (const i of imgs) { if (i.loading === 'lazy') i.loading = 'eager'; }
      const deadline = Date.now() + 8000;
      while (Date.now() < deadline && imgs.some((i) => !i.complete)) {
        await new Promise((r) => setTimeout(r, 150));
      }
    }).catch(() => {});
    const snap = await page.evaluate(SNAPSHOT);
    // On Linux Chromium the pageerror event often carries no frame at all,
    // while the console's own "Uncaught …" line for the same error has the
    // URL and line. Give the frameless entry that location.
    for (const e of consoleErrors) {
      if (e.url || !/^uncaught: /.test(e.text)) continue;
      const msg = e.text.slice('uncaught: '.length, 'uncaught: '.length + 80);
      const twin = consoleErrors.find((c) => c !== e && c.url && !/^uncaught: /.test(c.text) && c.text.includes(msg));
      if (twin) { e.url = twin.url; e.line = twin.line || e.line || 0; }
    }
    // Enrich type-name-only rejections with the contents recorded above, in
    // the order they happened.
    try {
      const rej = await page.evaluate(() => (window.__flRej || []).slice());
      for (const e of consoleErrors) {
        if (rej.length && /^uncaught: (Array\(\d+\)|\[object Object\]|Object|undefined|null)$/.test(e.text)) e.text = 'uncaught: ' + rej.shift();
      }
    } catch (_) {}
    const seenDead = new Set();
    return {
      url, ...snap, scriptHosts: [...scriptHosts], contactedHosts: [...contactedHosts],
      consoleErrors: [...new Set(consoleErrors.map((e) => JSON.stringify(e)))].map((e) => JSON.parse(e)),
      deadRequests: deadRequests.filter((d) => {
        const k = d.host + d.path + d.why;
        return seenDead.has(k) ? false : seenDead.add(k);
      })
    };
  } finally {
    page.off('console', onConsole);
    page.off('pageerror', onPageError);
    page.off('response', onResponse);
    page.off('requestfailed', onFailed);
    page.off('request', onRequest);
  }
}

/**
 * Sample the live page twice and keep only what both samples agree on.
 *
 * This is the mechanism that makes the whole gate usable. Without it the
 * report is dominated by a store's own nondeterminism — the banner that
 * rotates, the "N people viewing" counter, the A/B bucket, the recommendation
 * carousel that reshuffles — and none of that is something the clone is
 * supposed to reproduce. Intersecting two independent loads removes it
 * automatically, with no per-store allowlist to maintain.
 */
export function stable(a, b) {
  const both = (xa, xb) => { const s = new Set(xb); return xa.filter((v) => s.has(v)); };
  const bothBy = (xa, xb, k) => { const s = new Set(xb.map(k)); return xa.filter((v) => s.has(k(v))); };
  return {
    url: a.url,
    title: a.title,
    headings: both(a.headings, b.headings),
    headingVendor: { ...b.headingVendor, ...a.headingVendor },
    headingDynamic: { ...b.headingDynamic, ...a.headingDynamic },
    headingSlide: { ...b.headingSlide, ...a.headingSlide },
    // Two live samples on different templates = an A/B test in progress.
    templateId: a.templateId === b.templateId ? (a.templateId || null) : null,
    templateVaries: !!(a.templateId && b.templateId && a.templateId !== b.templateId),
    landmarks: both(a.landmarks, b.landmarks),
    fontsInUse: both(a.fontsInUse, b.fontsInUse),
    fontsLoaded: both(a.fontsLoaded, b.fontsLoaded),
    controls: bothBy(a.controls, b.controls, (c) => c.key),
    optionGroups: bothBy(a.optionGroups, b.optionGroups, (g) => g.kind + ':' + g.name),
    navTargets: both(a.navTargets, b.navTargets),
    // The live site's OWN console errors. Storefronts throw constantly — a
    // widget racing its container, a vendor SDK double-initialising — and an
    // error the merchant's live page throws is not something the clone
    // introduced. Subtracting them is the same "live is the oracle" rule the
    // rest of this file runs on, applied to the error log.
    consoleErrors: [...new Set([...a.consoleErrors, ...b.consoleErrors].map((e) => e.text))],
    scriptHosts: [...new Set([...(a.scriptHosts || []), ...(b.scriptHosts || [])])],
    contactedHosts: [...new Set([...(a.contactedHosts || []), ...(b.contactedHosts || [])])],
    // Media counts vary with lazy-loading timing, so the requirement is the
    // SMALLER of the two samples: a floor the clone must clear, never a target
    // it must hit exactly.
    media: {
      imagesRendered: Math.min(a.media.imagesRendered, b.media.imagesRendered),
      backgroundImages: Math.min(a.media.backgroundImages, b.media.backgroundImages),
      videos: Math.min(a.media.videos, b.media.videos)
    },
    textLength: Math.min(a.textLength, b.textLength),
    domNodes: Math.min(a.domNodes, b.domNodes)
  };
}

/**
 * Actuate one stamped control and report whether the page responded.
 *
 * The baseline pass is the load-bearing part. A storefront's hero video,
 * marquee and count-up animations mutate the DOM continuously, so measuring
 * "did the DOM change after I clicked" alone reports every control as working
 * — including the ones that do nothing. Measuring the page's idle rate first,
 * over the same interval, is what makes the answer mean anything.
 *
 * Clicks are dispatched from inside the page rather than through Playwright's
 * actionability model on purpose: half these controls are deliberately
 * off-screen until opened (a cart drawer's contents, a mobile menu), and
 * Playwright would refuse to click them or scroll something else into view.
 * We are asking whether the handler runs, not whether a human could reach it
 * at this viewport.
 */
export async function actuate(page, ctlId, { wait = 900, key = null } = {}) {
  let present = await page.$(`[data-fl-ctl="${ctlId}"]`);
  // Stamps do not survive a navigation, and probing a cart control navigates
  // by design — so every probe AFTER the first one used to report "control no
  // longer in the DOM" and be scored as a broken feature. That is a harness
  // bug wearing a fidelity bug's clothes, and it is exactly the shape this
  // whole tool exists to stop shipping. Re-stamping and re-finding the control
  // by its role+name key makes the probe independent of what earlier probes
  // did to the page.
  if (!present && key) {
    const fresh = await page.evaluate(SNAPSHOT).catch(() => null);
    const again = fresh?.controls.find((c) => c.key === key);
    if (again) { ctlId = again.ctl; present = await page.$(`[data-fl-ctl="${ctlId}"]`); }
  }
  if (!present) return { ok: false, responded: false, why: 'control no longer in the DOM' };

  await page.evaluate(INSTALL_PROBE).catch(() => {});
  // Idle baseline over exactly the interval the click will be measured over.
  const idleA = await page.evaluate(() => window.__flProbe.snap());
  await page.waitForTimeout(wait);
  const idleB = await page.evaluate(() => window.__flProbe.snap());
  const baseline = idleB.mut - idleA.mut;

  const before = await page.evaluate(() => window.__flProbe.snap());
  const fired = await page.evaluate((id) => {
    const el = document.querySelector(`[data-fl-ctl="${id}"]`);
    if (!el) return false;
    // A <select> never responds to a click; it responds to a change. Same for
    // radios — the theme listens for change, not for the click that caused it.
    if (el.tagName === 'SELECT' && el.options.length > 1) {
      el.selectedIndex = (el.selectedIndex + 1) % el.options.length;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    }
    if (el.tagName === 'INPUT' && (el.type === 'radio' || el.type === 'checkbox')) {
      el.checked = el.type === 'checkbox' ? !el.checked : true;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    }
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {
      el.focus();
      el.value = 'olive';
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new KeyboardEvent('keyup', { key: 'e', bubbles: true }));
      return true;
    }
    el.scrollIntoView({ block: 'center' });
    el.click();
    return true;
  }, ctlId).catch(() => false);
  if (!fired) return { ok: false, responded: false, why: 'control vanished before it could be actuated' };

  await page.waitForTimeout(wait);
  const after = await page.evaluate(() => window.__flProbe.snap()).catch(() => null);
  if (!after) return { ok: true, responded: true, why: 'page navigated away' };
  const r = responded(before, after, baseline);
  return { ok: true, responded: r.yes, why: r.why, navigated: after.url !== before.url };
}
