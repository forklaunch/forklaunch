import { NotFoundError } from '@mikro-orm/core';
import { array, handlers, number, schemaValidator } from '../../schema';
import { ImportProductSchema } from '../../domain/schemas/catalogImport.schema';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const productServiceFactory = ci.scopedResolver(tokens.ProductService);
const variantServiceFactory = ci.scopedResolver(tokens.VariantService);
const inventoryServiceFactory = ci.scopedResolver(tokens.InventoryService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

/**
 * The bulk catalog-import door — this is the one API surface Guild's
 * migration/clone tooling loads through (never raw SQL, per the ownership
 * boundary: we build the import API, Guild builds the tool that calls it).
 * Idempotent on externalId — re-running an import upserts, never duplicates.
 */
export const importCatalog = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Import Catalog',
    access: 'internal',
    summary: 'Bulk import a normalized product catalog',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: { products: array(ImportProductSchema) },
    responses: {
      200: { productsImported: number, variantsImported: number }
    }
  },
  async (req, res) => {
    let productsImported = 0;
    let variantsImported = 0;

    for (const product of req.body.products) {
      const productService = productServiceFactory();
      const productFields = {
        externalId: product.externalId,
        handle: product.handle,
        sourceUrl: product.sourceUrl,
        title: product.title,
        descriptionHtml: product.descriptionHtml,
        vendor: product.vendor,
        productType: product.productType,
        tags: product.tags,
        options: product.options,
        images: product.images
      };

      // Upsert by externalId — a re-run of the same import (retry, partial
      // failure recovery) must update the existing row, never duplicate it.
      let importedProduct;
      try {
        const existing = await productService.getProductByExternalId({
          externalId: product.externalId
        });
        importedProduct = await productService.updateProduct({
          id: existing.id,
          ...productFields
        });
      } catch (err) {
        if (!(err instanceof NotFoundError)) throw err;
        importedProduct = await productService.createProduct(productFields);
      }
      productsImported++;

      for (const variant of product.variants) {
        const variantService = variantServiceFactory();
        const variantFields = {
          productId: importedProduct.id,
          externalId: variant.externalId,
          sku: variant.sku,
          title: variant.title,
          optionValues: variant.optionValues,
          priceCents: variant.priceCents,
          compareAtPriceCents: variant.compareAtPriceCents,
          requiresShipping: variant.requiresShipping
        };

        let importedVariant;
        try {
          const existingVariant = await variantService.getVariantByExternalId({
            externalId: variant.externalId
          });
          importedVariant = await variantService.updateVariant({
            id: existingVariant.id,
            ...variantFields
          });
        } catch (err) {
          if (!(err instanceof NotFoundError)) throw err;
          importedVariant = await variantService.createVariant(variantFields);
        }

        // Inventory is seeded once at first import; re-imports never reset
        // live stock counts back to the source's snapshot.
        const inventoryService = inventoryServiceFactory();
        try {
          await inventoryService.getInventory({
            variantId: importedVariant.id
          });
        } catch (err) {
          if (!(err instanceof NotFoundError)) throw err;
          await inventoryService.createInventory({
            variantId: importedVariant.id,
            stock: variant.initialStock ?? 0
          });
        }
        variantsImported++;
      }
    }

    openTelemetryCollector.info('Catalog import complete', {
      productsImported,
      variantsImported
    });
    res.status(200).json({ productsImported, variantsImported });
  }
);
