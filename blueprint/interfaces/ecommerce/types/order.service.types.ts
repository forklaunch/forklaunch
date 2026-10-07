import { IdDto, IdsDto, RecordTimingDto } from '@forklaunch/common';

/** The order lifecycle. Cancellation is legal from any pre-terminal state. */
export const OrderStatus = {
  PENDING: 'pending',
  PAID: 'paid',
  FULFILLED: 'fulfilled',
  SHIPPED: 'shipped',
  DELIVERED: 'delivered',
  CANCELLED: 'cancelled'
} as const;

export type OrderStatusType = (typeof OrderStatus)[keyof typeof OrderStatus];

export type OrderItemDto = {
  variantId: string;
  quantity: number;
  unitPriceCents: number;
};

/** Ship-to address — required to compute both tax (jurisdiction) and
 *  shipping cost (rate zone) for a real, non-stubbed checkout. */
export type ShippingAddressDto = {
  name: string;
  line1: string;
  line2?: string;
  city: string;
  state: string;
  postalCode: string;
  country: string;
};

/** Per-jurisdiction tax line, persisted for reconciliation/filing —
 *  not just the total, per the tax-compliance guide's audit-trail rule. */
export type TaxLineDto = {
  jurisdiction: string;
  taxCents: number;
};

export type CreateOrderDto = Partial<IdDto> & {
  customerId?: string;
  items: OrderItemDto[];
  shippingAddress: ShippingAddressDto;
  subtotalCents: number;
  /** Promo-code discount, applied before tax. 0 when no code was used. */
  discountCents: number;
  taxCents: number;
  taxBreakdown: TaxLineDto[];
  shippingCents: number;
  /** Gift-card amount applied against the final total (tender, not a
   *  pre-tax discount). 0 when no gift card was used. */
  giftCardCents: number;
  totalCents: number;
};

export type UpdateOrderDto = Partial<IdDto> & {
  id: string;
  status?: OrderStatusType;
};

export type OrderDto = CreateOrderDto &
  IdDto &
  Partial<RecordTimingDto> & {
    status: OrderStatusType;
  };

/** A requested status transition; the service enforces legality. */
export type TransitionOrderDto = {
  id: string;
  to: OrderStatusType;
};

export type OrderServiceParameters = {
  CreateOrderDto: CreateOrderDto;
  UpdateOrderDto: UpdateOrderDto;
  OrderDto: OrderDto;
  TransitionOrderDto: TransitionOrderDto;
  IdDto: IdDto;
  IdsDto: IdsDto;
};
