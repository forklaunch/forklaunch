import { EntityManager } from '@mikro-orm/core';
import { OrderServiceParameters } from '../types/order.service.types';

export interface OrderService<
  Params extends OrderServiceParameters = OrderServiceParameters
> {
  createOrder: (
    orderDto: Params['CreateOrderDto'],
    em?: EntityManager
  ) => Promise<Params['OrderDto']>;
  getOrder: (
    idDto: Params['IdDto'],
    em?: EntityManager
  ) => Promise<Params['OrderDto']>;
  listOrders: (
    idsDto?: Params['IdsDto'],
    em?: EntityManager
  ) => Promise<Params['OrderDto'][]>;
  /** Rejects illegal transitions per the ECOM-07 state machine. */
  transitionOrder: (
    transitionDto: Params['TransitionOrderDto'],
    em?: EntityManager
  ) => Promise<Params['OrderDto']>;
}
