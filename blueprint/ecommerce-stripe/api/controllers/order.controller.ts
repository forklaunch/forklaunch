import { randomUUID } from 'node:crypto';
import {
  array,
  enum_,
  handlers,
  IdSchema,
  IdsSchema,
  schemaValidator,
  string
} from '../../schema';
import { OrderStatus } from '@forklaunch/interfaces-ecommerce/types';
import { IllegalOrderTransitionError } from '@forklaunch/implementation-ecommerce-base/services';
import { ci, tokens } from '../../bootstrapper';
import {
  CreateOrderMapper,
  OrderMapper
} from '../../domain/mappers/order.mappers';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.OrderService);
const orderEventProducerFactory = ci.scopedResolver(tokens.OrderEventProducer);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const createOrder = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Order',
    access: 'internal',
    summary: 'Create an order',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: CreateOrderMapper.schema,
    responses: { 200: OrderMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().createOrder(req.body));
  }
);

export const getOrder = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Order',
    access: 'internal',
    summary: 'Get an order',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: OrderMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().getOrder(req.params));
  }
);

export const listOrders = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Orders',
    access: 'internal',
    summary: 'List orders',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    query: IdsSchema,
    responses: { 200: array(OrderMapper.schema) }
  },
  async (req, res) => {
    res
      .status(200)
      .json(
        await serviceFactory().listOrders(
          req.query.ids ? (req.query as { ids: string[] }) : undefined
        )
      );
  }
);

/**
 * The state-machine transition endpoint — rejects illegal
 * transitions with a 400. Each successful transition enqueues an
 * OrderEventRecord — the actual event-emission boundary worker.ts consumes,
 * previously just a comment here rather than real code.
 */
export const transitionOrder = handlers.put(
  schemaValidator,
  '/:id/transition',
  {
    name: 'Transition Order',
    access: 'internal',
    summary: 'Transition an order to a new status',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    body: { to: enum_(OrderStatus) },
    responses: { 200: OrderMapper.schema, 400: string }
  },
  async (req, res) => {
    const before = await serviceFactory().getOrder({ id: req.params.id });

    let updated;
    try {
      updated = await serviceFactory().transitionOrder({
        id: req.params.id,
        to: req.body.to
      });
    } catch (error) {
      // Only a rejected transition is the caller's fault. A database that is
      // unreachable, or a mapper that blows up, has to reach the framework
      // error handler and surface as a 5xx; reporting it as 400 hides a
      // service fault from every dashboard that watches for them.
      if (error instanceof IllegalOrderTransitionError) {
        openTelemetryCollector.warn('Illegal order transition', error);
        return res.status(400).send(error.message);
      }
      throw error;
    }

    const now = new Date();
    await orderEventProducerFactory().enqueueJob({
      id: randomUUID(),
      orderId: updated.id,
      fromStatus: before.status,
      toStatus: updated.status,
      items: updated.items,
      processed: false,
      retryCount: 0,
      retentionAnonymizedAt: null,
      createdAt: now,
      updatedAt: now
    });

    res.status(200).json(updated);
  }
);
