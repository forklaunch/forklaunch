import {
  boolean,
  handlers,
  number,
  schemaValidator,
  string
} from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import { InventoryMapper } from '../../domain/mappers/inventory.mappers';

const serviceFactory = ci.scopedResolver(tokens.InventoryService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const getInventory = handlers.get(
  schemaValidator,
  '/:variantId',
  {
    name: 'Get Inventory',
    access: 'internal',
    summary: 'Get inventory for a variant',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: { variantId: string },
    responses: { 200: InventoryMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().getInventory(req.params));
  }
);

/** Manual stock adjustment — guards against oversell at the service layer. */
export const adjustStock = handlers.put(
  schemaValidator,
  '/adjust',
  {
    name: 'Adjust Stock',
    access: 'internal',
    summary: 'Adjust stock for a variant',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: { variantId: string, delta: number },
    responses: { 200: InventoryMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().adjustStock(req.body));
  }
);

/** The synchronous "in stock?" check checkout depends on. */
export const checkStock = handlers.post(
  schemaValidator,
  '/check',
  {
    name: 'Check Stock',
    access: 'internal',
    summary: 'Check whether requested quantity is available',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: { variantId: string, requested: number },
    responses: {
      200: { variantId: string, available: boolean, stock: number }
    }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().checkStock(req.body));
  }
);
