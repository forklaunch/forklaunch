import type {
  NormalizedCatalog,
  NormalizedProduct,
  NormalizedVariant,
  NormalizedOption,
  GapNote,
} from './model.ts';

/** Product types that are not real physical catalog products for our purposes. */
const JUNK_PRODUCT_TYPES = new Set(['Gift Cards', 'Insurance', 'Warranty', 'Donation']);

/** Title substrings that flag non-product / test entries. */
const JUNK_TITLE_HINTS = ['package protection', 'gift card', 'donation', 'test product', 'do not buy'];

function toCents(price: string | number | null | undefined): number | null {
  if (price === null || price === undefined || price === '') return null;
  const n = typeof price === 'number' ? price : parseFloat(price);
  if (Number.isNaN(n)) return null;
  return Math.round(n * 100);
}

/**
 * Decide whether a "Size"-named option is really pack quantity (Olipop case:
 * "1 can" / "4-pack" / "12-pack") rather than a physical size.
 */
function looksLikePackQuantity(optionName: string, values: string[]): boolean {
  if (!/^size$/i.test(optionName.trim())) return false;
  const packish = values.filter((v) => /pack|count|\bcans?\b|\bct\b|x\s*\d|\d+\s*-?\s*pack/i.test(v));
  return packish.length >= Math.ceil(values.length / 2);
}

export interface NormalizeResult {
  catalog: NormalizedCatalog;
  gaps: GapNote[];
}

export function normalizeShopify(raw: any, shop: string, sourceBaseUrl: string): NormalizeResult {
  const gaps: GapNote[] = [];
  const rawProducts: any[] = raw.products ?? [];
  const kept: NormalizedProduct[] = [];
  let filtered = 0;

  for (const p of rawProducts) {
    const handle: string = p.handle ?? '';
    const productType: string = p.product_type ?? '';
    const variantsRaw: any[] = p.variants ?? [];

    // --- Junk / non-product filtering ---
    const titleLc = (p.title ?? '').toLowerCase();
    const isJunkType = JUNK_PRODUCT_TYPES.has(productType);
    const isJunkTitle = JUNK_TITLE_HINTS.some((h) => titleLc.includes(h));
    const allNonShipping = variantsRaw.length > 0 && variantsRaw.every((v) => v.requires_shipping === false);
    if (isJunkType || isJunkTitle || (allNonShipping && productType !== '')) {
      filtered++;
      gaps.push({
        kind: 'filtered-non-product',
        detail: `Dropped "${p.title}" (type="${productType}", requires_shipping-all-false=${allNonShipping})`,
        productHandle: handle,
      });
      continue;
    }

    // --- Option normalization (clean names, detect pack-quantity) ---
    const rawOptions: any[] = p.options ?? [];
    const options: NormalizedOption[] = rawOptions.map((o) => {
      const cleanName = String(o.name ?? '').trim();
      if (cleanName !== o.name) {
        gaps.push({
          kind: 'option-name-cleaned',
          detail: `Option name "${o.name}" cleaned to "${cleanName}" on "${p.title}"`,
          productHandle: handle,
        });
      }
      const values: string[] = o.values ?? [];
      const isPackQuantity = looksLikePackQuantity(cleanName, values);
      if (isPackQuantity) {
        gaps.push({
          kind: 'size-is-pack-quantity',
          detail: `Option "Size" on "${p.title}" is pack-quantity, not physical size (${values.join(', ')})`,
          productHandle: handle,
        });
      }
      return { name: cleanName, isPackQuantity, values };
    });
    const cleanOptionNames = options.map((o) => o.name);

    // --- Variants ---
    const variants: NormalizedVariant[] = variantsRaw.map((v) => {
      const optionValues: Record<string, string> = {};
      [v.option1, v.option2, v.option3].forEach((val, i) => {
        if (val != null && cleanOptionNames[i]) optionValues[cleanOptionNames[i]] = val;
      });
      const priceCents = toCents(v.price) ?? 0;
      if (toCents(v.price) === null) {
        gaps.push({
          kind: 'missing-price',
          detail: `Variant "${v.title}" on "${p.title}" had no parseable price; defaulted to 0`,
          productHandle: handle,
        });
      }
      if (!v.sku) {
        gaps.push({
          kind: 'missing-sku',
          detail: `Variant "${v.title}" on "${p.title}" has no SKU`,
          productHandle: handle,
        });
      }
      return {
        externalId: String(v.id),
        sku: v.sku ?? '',
        title: v.title ?? '',
        optionValues,
        priceCents,
        compareAtPriceCents: toCents(v.compare_at_price),
        available: v.available ?? true,
        // Public feed has no stock counts — an Admin pull fills this instead.
        inventoryQuantity: null,
        requiresShipping: v.requires_shipping ?? true,
        grams: v.grams ?? 0,
      };
    });

    kept.push({
      externalId: String(p.id),
      handle,
      sourceUrl: `${sourceBaseUrl.replace(/\/$/, '')}/products/${handle}`,
      title: p.title ?? '',
      descriptionHtml: p.body_html ?? '',
      vendor: p.vendor ?? '',
      productType,
      tags: p.tags ?? [],
      options,
      images: (p.images ?? []).map((img: any) => ({ src: img.src, position: img.position ?? 0 })),
      variants,
    });
  }

  // Subscriptions are configured via Shopify "selling plans", which the public
  // products.json feed does NOT expose. Flag it once as a known gap.
  gaps.push({
    kind: 'subscriptions-not-in-feed',
    detail:
      'Subscribe-and-save (selling plans) is not present in the public products.json feed. ' +
      'Subscription config must come from another source (merchant export / admin API).',
  });

  const catalog: NormalizedCatalog = {
    source: {
      shop,
      sourceUrl: sourceBaseUrl,
      platform: 'shopify',
      pulledAt: raw.__pulledAt ?? 'unknown',
      rawProductCount: rawProducts.length,
      keptProductCount: kept.length,
      filteredProductCount: filtered,
    },
    products: kept,
  };

  return { catalog, gaps };
}
