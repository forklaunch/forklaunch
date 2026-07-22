import { EntityManager } from '@mikro-orm/core';
import { CartServiceParameters } from '../types/cart.service.types';

export interface CartService<
  Params extends CartServiceParameters = CartServiceParameters
> {
  createCart: (
    cartDto: Params['CreateCartDto'],
    em?: EntityManager
  ) => Promise<Params['CartDto']>;
  getCart: (
    idDto: Params['IdDto'],
    em?: EntityManager
  ) => Promise<Params['CartDto']>;
  addItem: (
    addItemDto: Params['AddCartItemDto'],
    em?: EntityManager
  ) => Promise<Params['CartDto']>;
  removeItem: (
    removeItemDto: Params['RemoveCartItemDto'],
    em?: EntityManager
  ) => Promise<Params['CartDto']>;
  clearCart: (idDto: Params['IdDto'], em?: EntityManager) => Promise<void>;
}
