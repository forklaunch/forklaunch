import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  createVariant,
  deleteVariant,
  getVariant,
  listVariants,
  listVariantsByProduct,
  updateVariant
} from '../controllers/variant.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const variantRouter = forklaunchRouter(
  '/variant',
  schemaValidator,
  openTelemetryCollector
);

export const createVariantRoute = variantRouter.post('/', createVariant);
export const listVariantsRoute = variantRouter.get('/', listVariants);
export const listVariantsByProductRoute = variantRouter.get(
  '/product/:productId',
  listVariantsByProduct
);
export const getVariantRoute = variantRouter.get('/:id', getVariant);
export const updateVariantRoute = variantRouter.put('/', updateVariant);
export const deleteVariantRoute = variantRouter.delete('/:id', deleteVariant);
