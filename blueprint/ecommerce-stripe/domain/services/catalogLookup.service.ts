import { EntityManager } from '@mikro-orm/core';
import { Inventory } from '../../persistence/entities/inventory.entity';
import { Variant } from '../../persistence/entities/variant.entity';

/**
 * The two bulk reads a catalog listing needs.
 *
 * Rendering a storefront page means knowing, for a set of products, their
 * variants and each variant's stock. The base package exposes neither of
 * those as a set operation — listVariantsByProduct takes one product and
 * getInventory takes one variant — so every consumer that renders a catalog
 * is pushed into an N+1: on a 79-product store, 79 requests for variants and
 * 143 for stock, each with its own HMAC signature and database round trip.
 *
 * Lives as its own small service for the same reason OrderCartLookupService
 * and PaymentOrderLookupService do: these queries do not exist on the shared
 * base package, and that package releases independently, so adding to it
 * would couple this to a publish. Querying the concrete entities here keeps
 * it local.
 */
export class CatalogLookupService {
  constructor(private readonly em: EntityManager) {}

  /**
   * The ids of every variant belonging to any of these products, in one
   * query.
   *
   * Ids rather than whole rows deliberately: the caller hands them to
   * VariantService, which owns the entity-to-DTO mapping every other variant
   * endpoint goes through. Returning raw entities here would put a second,
   * subtly different shape of a variant into the API — the persisted row has
   * nullable columns where the DTO has optional fields.
   *
   * Scoped to the requested products rather than reading the whole table and
   * filtering in memory: a page shows a couple of dozen products, and the
   * point of this service is that cost tracks what is displayed rather than
   * how large the merchant's catalog is.
   */
  async findVariantIdsByProductIds(productIds: string[]): Promise<string[]> {
    if (!productIds.length) {
      return [];
    }
    const rows = await this.em.find(
      Variant,
      { productId: { $in: productIds } },
      { fields: ['id'] }
    );
    return rows.map((row) => row.id);
  }

  /**
   * Stock keyed by variant id, for the given variants, in one query.
   *
   * Variants with no inventory row are absent from the result rather than
   * reported as zero — "no record" and "recorded as none in stock" are
   * different states, and only the caller knows which its display should
   * treat as unavailable. A store that has never tracked stock should not
   * have every product render as sold out.
   */
  async findStockByVariantIds(
    variantIds: string[]
  ): Promise<Record<string, number>> {
    if (!variantIds.length) {
      return {};
    }
    const rows = await this.em.find(Inventory, {
      variantId: { $in: variantIds }
    });
    return Object.fromEntries(rows.map((row) => [row.variantId, row.stock]));
  }
}
