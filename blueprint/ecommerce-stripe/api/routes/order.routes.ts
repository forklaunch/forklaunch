import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  createOrder,
  getOrder,
  listOrders,
  transitionOrder
} from '../controllers/order.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const orderRouter = forklaunchRouter(
  '/order',
  schemaValidator,
  openTelemetryCollector
);

export const createOrderRoute = orderRouter.post('/', createOrder);
export const listOrdersRoute = orderRouter.get('/', listOrders);
export const getOrderRoute = orderRouter.get('/:id', getOrder);
export const transitionOrderRoute = orderRouter.put(
  '/:id/transition',
  transitionOrder
);
