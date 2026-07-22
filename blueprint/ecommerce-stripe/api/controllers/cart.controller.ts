import {
  handlers,
  IdSchema,
  number,
  schemaValidator,
  string
} from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  CartMapper,
  CreateCartMapper
} from '../../domain/mappers/cart.mappers';

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

/** Out-of-stock items are allowed in the cart — checked at checkout, not here. */
export const addCartItem = handlers.post(
  schemaValidator,
  '/items',
  {
    name: 'Add Cart Item',
    access: 'internal',
    summary: 'Add an item to a cart',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: { cartId: string, variantId: string, quantity: number },
    responses: { 200: CartMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().addItem(req.body));
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
