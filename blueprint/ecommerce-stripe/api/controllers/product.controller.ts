import {
  array,
  boolean,
  handlers,
  IdSchema,
  number,
  optional,
  schemaValidator,
  string
} from '../../schema';
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
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const ProductSearchQuerySchema = {
  ids: optional(array(string)),
  title: optional(string),
  minPriceCents: optional(number),
  maxPriceCents: optional(number),
  inStock: optional(boolean),
  optionName: optional(string),
  optionValue: optional(string)
};

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
    res
      .status(200)
      .json(await serviceFactory().getProductByHandle(req.params));
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
 * Catalog search/filter (ECOM-03). `ids`/`title` are product-level and
 * resolved directly by the product service. `minPriceCents`/`maxPriceCents`/
 * `inStock`/`optionName`+`optionValue` are variant-level — Product itself
 * carries no price — so they're resolved here by narrowing variants first,
 * then constraining the product query to the productIds that survive.
 */
export const listProducts = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Products',
    access: 'internal',
    summary: 'List/search products, optionally filtered by title, price, stock, or a variant option value',
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
      if (inStock) {
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
        candidateVariants = candidateVariants.filter((_, i) => stockChecks[i]);
      }

      const candidateProductIds = [
        ...new Set(candidateVariants.map((v) => v.productId))
      ];
      productIds = ids?.length
        ? ids.filter((id) => candidateProductIds.includes(id))
        : candidateProductIds;
    }

    res.status(200).json(
      await serviceFactory().listProducts({
        ...(productIds ? { ids: productIds } : {}),
        ...(title ? { title } : {})
      })
    );
  }
);
