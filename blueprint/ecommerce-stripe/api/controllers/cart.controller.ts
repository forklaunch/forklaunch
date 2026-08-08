import {
  handlers,
  IdSchema,
  number,
  schemaValidator,
  string
} from '../../schema';
import { CartValidationError } from '@forklaunch/implementation-ecommerce-base/services';
import { ci, tokens } from '../../bootstrapper';
import {
  CartMapper,
  CreateCartMapper
} from '../../domain/mappers/cart.mappers';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.CartService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const createCart = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Cart',
    access: 'internal',
    summary: 'Create a cart',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: CreateCartMapper.schema,
    responses: { 200: CartMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().createCart(req.body));
  }
);

export const getCart = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Cart',
    access: 'internal',
    summary: 'Get a cart',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: CartMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().getCart(req.params));
  }
);

/**
 * Out-of-stock items are allowed in the cart — checked at checkout, not
 * here. Quantity validity (positive integer) is not, though: addItem
 * throws a CartValidationError for a bad quantity, same reasoning as
 * order.controller.ts's transitionOrder mapping IllegalOrderTransitionError
 * to a 400 rather than letting it fall through to the generic handler as
 * an opaque 500. Only CartValidationError is mapped this way — anything
 * else (a genuine infra failure) still 500s.
 */
export const addCartItem = handlers.post(
  schemaValidator,
  '/items',
  {
    name: 'Add Cart Item',
    access: 'internal',
    summary: 'Add an item to a cart',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: { cartId: string, variantId: string, quantity: number },
    responses: { 200: CartMapper.schema, 400: string }
  },
  async (req, res) => {
    try {
      res.status(200).json(await serviceFactory().addItem(req.body));
    } catch (error) {
      if (error instanceof CartValidationError) {
        openTelemetryCollector.warn('Invalid cart item quantity', error);
        res.status(400).send(error.message);
        return;
      }
      throw error;
    }
  }
);

export const removeCartItem = handlers.delete(
  schemaValidator,
  '/:cartId/items/:variantId',
  {
    name: 'Remove Cart Item',
    access: 'internal',
    summary: 'Remove an item from a cart',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: { cartId: string, variantId: string },
    responses: { 200: CartMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().removeItem(req.params));
  }
);

export const clearCart = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Clear Cart',
    access: 'internal',
    summary: 'Clear all items from a cart',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: string }
  },
  async (req, res) => {
    await serviceFactory().clearCart(req.params);
    res.status(200).send('Cleared cart');
  }
);
