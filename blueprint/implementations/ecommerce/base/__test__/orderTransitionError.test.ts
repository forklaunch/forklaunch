import { describe, expect, it } from 'vitest';
import { OrderStatus } from '@forklaunch/interfaces-ecommerce/types';
import { IllegalOrderTransitionError } from '../services/order.service';

// The order controller distinguishes a rejected transition from an
// infrastructure failure with `instanceof IllegalOrderTransitionError`, and
// returns 400 for the former while letting the latter reach the framework
// handler as a 5xx. If this type stops being identifiable, every illegal
// transition silently becomes a 500 and callers start retrying a request that
// can never succeed.
describe('IllegalOrderTransitionError', () => {
  it('is identifiable by instanceof', () => {
    const error = new IllegalOrderTransitionError(
      OrderStatus.SHIPPED,
      OrderStatus.PAID
    );
    expect(error).toBeInstanceOf(IllegalOrderTransitionError);
    expect(error).toBeInstanceOf(Error);
  });

  it('keeps a stable name', () => {
    expect(
      new IllegalOrderTransitionError(OrderStatus.SHIPPED, OrderStatus.PAID)
        .name
    ).toBe('IllegalOrderTransitionError');
  });

  it('names both states in the message, which is sent to the caller verbatim', () => {
    expect(
      new IllegalOrderTransitionError(
        OrderStatus.SHIPPED,
        OrderStatus.CANCELLED
      ).message
    ).toBe(
      `Illegal order transition: ${OrderStatus.SHIPPED} -> ${OrderStatus.CANCELLED}`
    );
  });
});
