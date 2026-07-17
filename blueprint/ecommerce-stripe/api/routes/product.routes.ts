import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  createProduct,
  deleteProduct,
  getProduct,
  getProductByHandle,
  listProducts,
  updateProduct
} from '../controllers/product.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const productRouter = forklaunchRouter(
  '/product',
  schemaValidator,
  openTelemetryCollector
);

export const createProductRoute = productRouter.post('/', createProduct);
export const listProductsRoute = productRouter.get('/', listProducts);
export const getProductByHandleRoute = productRouter.get(
  '/handle/:handle',
  getProductByHandle
);
export const getProductRoute = productRouter.get('/:id', getProduct);
export const updateProductRoute = productRouter.put('/', updateProduct);
export const deleteProductRoute = productRouter.delete('/:id', deleteProduct);
