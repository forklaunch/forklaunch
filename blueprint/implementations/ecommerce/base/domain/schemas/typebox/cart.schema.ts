import {
  array,
  number,
  optional,
  string,
  uuid
} from '@forklaunch/validator/typebox';

const CartItemSchema = {
  variantId: string,
  quantity: number
};

export const CreateCartSchema = {
  customerId: optional(string)
};

export const UpdateCartSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  customerId: optional(string)
});

export const CartSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  customerId: optional(string),
  status: string,
  items: array(CartItemSchema)
});

export const AddCartItemSchema = {
  cartId: string,
  variantId: string,
  quantity: number
};

export const RemoveCartItemSchema = {
  cartId: string,
  variantId: string
};

export const BaseCartServiceSchemas = (options: { uuidId: boolean }) => ({
  CreateCartSchema,
  UpdateCartSchema: UpdateCartSchema(options),
  CartSchema: CartSchema(options)
});
