import {
  array,
  date,
  enum_,
  number,
  optional,
  string,
  uuid
} from '@forklaunch/validator/typebox';

const OrderStatusEnum = {
  PENDING: 'pending',
  PAID: 'paid',
  FULFILLED: 'fulfilled',
  SHIPPED: 'shipped',
  DELIVERED: 'delivered',
  CANCELLED: 'cancelled'
} as const;

const OrderItemSchema = {
  variantId: string,
  quantity: number,
  unitPriceCents: number
};

const ShippingAddressSchema = {
  name: string,
  line1: string,
  line2: optional(string),
  city: string,
  state: string,
  postalCode: string,
  country: string
};

const TaxLineSchema = {
  jurisdiction: string,
  taxCents: number
};

export const CreateOrderSchema = {
  customerId: optional(string),
  items: array(OrderItemSchema),
  shippingAddress: ShippingAddressSchema,
  subtotalCents: number,
  discountCents: number,
  taxCents: number,
  taxBreakdown: array(TaxLineSchema),
  shippingCents: number,
  giftCardCents: number,
  totalCents: number
};

export const UpdateOrderSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  status: optional(enum_(OrderStatusEnum))
});

export const OrderSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  customerId: optional(string),
  status: enum_(OrderStatusEnum),
  items: array(OrderItemSchema),
  shippingAddress: ShippingAddressSchema,
  subtotalCents: number,
  discountCents: number,
  taxCents: number,
  taxBreakdown: array(TaxLineSchema),
  shippingCents: number,
  giftCardCents: number,
  totalCents: number,
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const TransitionOrderSchema = {
  id: string,
  to: enum_(OrderStatusEnum)
};

export const BaseOrderServiceSchemas = (options: { uuidId: boolean }) => ({
  CreateOrderSchema,
  UpdateOrderSchema: UpdateOrderSchema(options),
  OrderSchema: OrderSchema(options)
});
