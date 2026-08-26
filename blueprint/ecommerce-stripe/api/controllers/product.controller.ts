import {
  array,
  handlers,
  IdSchema,
  number,
  optional,
  schemaValidator,
  string
} from '../../schema';
import { ProductSearchQuerySchema } from '../../domain/schemas/productSearch.schema';
import { CatalogPageSchema } from '../../domain/schemas/catalog.schema';
import { ci, tokens } from '../../bootstrapper';
import {
  CreateProductMapper,
  ProductMapper,
  UpdateProductMapper
} from '../../domain/mappers/product.mappers';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.ProductService);
const variantServiceFactory = ci.scopedResolver(tokens.VariantService);
const inventoryServiceFactory = ci.scopedResolver(tokens.InventoryService);
const catalogLookupServiceFactory = ci.scopedResolver(
  tokens.CatalogLookupService
);

/**
 * Cap on how many products one catalog request may return. A storefront
 * asking for more than this is almost certainly trying to render its whole
 * catalog in one page, which is the behaviour this endpoint exists to
 * replace — not something to serve faster.
 */
const CATALOG_MAX_LIMIT = 100;
const CATALOG_DEFAULT_LIMIT = 24;
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const createProduct = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Product',
    access: 'internal',
    summary: 'Create a product',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: CreateProductMapper.schema,
    responses: { 200: ProductMapper.schema }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating product', req.body);
    res.status(200).json(await serviceFactory().createProduct(req.body));
  }
);

export const getProduct = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Product',
    access: 'internal',
    summary: 'Get a product',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: ProductMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().getProduct(req.params));
  }
);

export const getProductByHandle = handlers.get(
  schemaValidator,
  '/handle/:handle',
  {
    name: 'Get Product By Handle',
    access: 'internal',
    summary: 'Get a product by its URL handle',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: { handle: string },
    responses: { 200: ProductMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().getProductByHandle(req.params));
  }
);

export const updateProduct = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update Product',
    access: 'internal',
    summary: 'Update a product',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: UpdateProductMapper.schema,
    responses: { 200: ProductMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().updateProduct(req.body));
  }
);

export const deleteProduct = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Product',
    access: 'internal',
    summary: 'Delete a product',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: string }
  },
  async (req, res) => {
    await serviceFactory().deleteProduct(req.params);
    res.status(200).send('Deleted product');
  }
);

/**
 * Catalog search/filter. `ids`/`title` are product-level and
 * resolved directly by the product service. `minPriceCents`/`maxPriceCents`/
 * `inStock`/`optionName`+`optionValue` are variant-level — Product itself
 * carries no price — so they're resolved here by narrowing variants first,
 * then constraining the product query to the productIds that survive.
 */
/**
 * The whole catalog page in one request.
 *
 * Rendering a storefront listing previously meant one call for the products,
 * one per product for its variants, and one per variant for its stock — 223
 * HMAC-signed round trips for a 79-product store, each its own database hit.
 * Nothing about the data requires that: products, variants and stock are
 * three queries, and joining them here once is cheaper than a client doing
 * it N times over HTTP.
 *
 * Deliberately paginated. Unpaginated this would still be one request, but
 * its cost would track the size of the merchant's catalog rather than what
 * the page displays — the same failure one layer along.
 *
 * Filtering is product-level only (`ids`, `title`). The variant-level
 * filters on GET /product — price, stock, option values — need every variant
 * in the store loaded before they can select products, which is the cost
 * this endpoint exists to avoid. A storefront that needs them can search
 * there for ids and pass those here.
 */
export const listCatalog = handlers.get(
  schemaValidator,
  '/catalog',
  {
    name: 'List Catalog',
    access: 'internal',
    summary:
      'Products with their variants and stock in one paginated call — the storefront listing read',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    query: {
      ids: optional(array(string)),
      title: optional(string),
      limit: optional(number),
      offset: optional(number)
    },
    responses: { 200: CatalogPageSchema }
  },
  async (req, res) => {
    const { ids, title } = req.query;
    const limit = Math.min(
      Math.max(req.query.limit ?? CATALOG_DEFAULT_LIMIT, 1),
      CATALOG_MAX_LIMIT
    );
    const offset = Math.max(req.query.offset ?? 0, 0);

    const products = await serviceFactory().listProducts(
      ids ? { ids } : undefined
    );
    const matched =
      title != null
        ? products.filter((product) =>
            product.title.toLowerCase().includes(title.toLowerCase())
          )
        : products;
    const page = matched.slice(offset, offset + limit);

    // Two queries for the whole page, however many products it holds.
    const catalogLookup = catalogLookupServiceFactory();
    const variantIds = await catalogLookup.findVariantIdsByProductIds(
      page.map((product) => product.id)
    );
    const [variants, stock] = await Promise.all([
      variantIds.length
        ? variantServiceFactory().listVariants({ ids: variantIds })
        : Promise.resolve([]),
      catalogLookup.findStockByVariantIds(variantIds)
    ]);

    const variantsByProduct = new Map<string, typeof variants>();
    for (const variant of variants) {
      const bucket = variantsByProduct.get(variant.productId);
      if (bucket) {
        bucket.push(variant);
      } else {
        variantsByProduct.set(variant.productId, [variant]);
      }
    }

    res.status(200).json({
      products: page.map((product) => ({
        ...product,
        variants: (variantsByProduct.get(product.id) ?? []).map((variant) => ({
          ...variant,
          stock: stock[variant.id]
        }))
      })),
      total: matched.length,
      limit,
      offset
    });
  }
);

export const listProducts = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Products',
    access: 'internal',
    summary:
      'List/search products, optionally filtered by title, price, stock, or a variant option value',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    query: ProductSearchQuerySchema,
    responses: { 200: array(ProductMapper.schema) }
  },
  async (req, res) => {
    const {
      ids,
      title,
      minPriceCents,
      maxPriceCents,
      inStock,
      optionName,
      optionValue
    } = req.query;

    const hasVariantLevelFilter =
      minPriceCents != null ||
      maxPriceCents != null ||
      inStock != null ||
      (optionName != null && optionValue != null);

    let productIds = ids;

    if (hasVariantLevelFilter) {
      let candidateVariants = await variantServiceFactory().listVariants();

      if (minPriceCents != null) {
        candidateVariants = candidateVariants.filter(
          (v) => v.priceCents >= minPriceCents
        );
      }
      if (maxPriceCents != null) {
        candidateVariants = candidateVariants.filter(
          (v) => v.priceCents <= maxPriceCents
        );
      }
      if (optionName != null && optionValue != null) {
        candidateVariants = candidateVariants.filter(
          (v) => v.optionValues?.[optionName] === optionValue
        );
      }
      // `!= null`, not truthiness: `inStock=false` is a real filter for
      // out-of-stock variants. Testing `if (inStock)` skipped the check
      // entirely on `false`, so the endpoint returned variants regardless of
      // stock while implying it had filtered them.
      if (inStock != null) {
        const stockChecks = await Promise.all(
          candidateVariants.map(async (v) => {
            try {
              const inventory = await inventoryServiceFactory().getInventory({
                variantId: v.id
              });
              return inventory.stock > 0;
            } catch {
              // No inventory record yet — treat as out of stock, not an error.
              return false;
            }
          })
        );
        candidateVariants = candidateVariants.filter(
          (_, i) => stockChecks[i] === inStock
        );
      }

      const candidateProductIds = [
        ...new Set(candidateVariants.map((v) => v.productId))
      ];
      productIds = ids?.length
        ? ids.filter((id) => candidateProductIds.includes(id))
        : candidateProductIds;
    }

    // An empty productIds means the variant-level filters matched nothing —
    // a real, empty result, not the absence of a filter. It has to be
    // short-circuited here because neither layer below can tell those apart:
    // `[]` is truthy, so `{ ids: [] }` is forwarded; listProducts then tests
    // `searchDto?.ids?.length`, which is 0, so it never constrains the query
    // and returns the ENTIRE catalog. A shopper filtering for an out-of-stock
    // or non-existent option would be shown every product as if it matched.
    if (productIds != null && productIds.length === 0) {
      res.status(200).json([]);
      return;
    }

    res.status(200).json(
      await serviceFactory().listProducts({
        ...(productIds ? { ids: productIds } : {}),
        ...(title ? { title } : {})
      })
    );
  }
);
