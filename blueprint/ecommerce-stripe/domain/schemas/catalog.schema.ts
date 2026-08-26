import { array, number, optional } from '../../schema';
import { ProductMapper } from '../mappers/product.mappers';
import { VariantMapper } from '../mappers/variant.mappers';

/**
 * One product with everything a storefront needs to render its card: the
 * product, its variants, and the stock of each.
 *
 * `stock` is optional rather than defaulted to 0 because a variant with no
 * inventory row and a variant recorded as having none are different states
 * (see InventoryStockLookupService). A store that has never tracked stock
 * should not have every product display as sold out.
 */
export const CatalogProductSchema = {
  ...ProductMapper.schema,
  variants: array({
    ...VariantMapper.schema,
    stock: optional(number)
  })
};

/**
 * Paginated so the response size is bounded by the request rather than by
 * how large the merchant's catalog happens to be. `total` is the count
 * before the page was taken, which is what a storefront needs to render
 * pagination controls without a second call.
 */
export const CatalogPageSchema = {
  products: array(CatalogProductSchema),
  total: number,
  limit: number,
  offset: number
};
