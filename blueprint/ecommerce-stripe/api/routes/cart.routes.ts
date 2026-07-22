import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  addCartItem,
  clearCart,
  createCart,
  getCart,
  removeCartItem
} from '../controllers/cart.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const cartRouter = forklaunchRouter(
  '/cart',
  schemaValidator,
  openTelemetryCollector
);

export const createCartRoute = cartRouter.post('/', createCart);
export const addCartItemRoute = cartRouter.post('/items', addCartItem);
export const removeCartItemRoute = cartRouter.delete(
  '/:cartId/items/:variantId',
  removeCartItem
);
export const getCartRoute = cartRouter.get('/:id', getCart);
export const clearCartRoute = cartRouter.delete('/:id', clearCart);
