import { SchemaValidator } from '../../schema';
import { Schema } from '@forklaunch/validator';
import {
  Cart,
  Inventory,
  Order,
  Payment,
  Product,
  Variant
} from '../../persistence/entities';
import {
  CartMapper,
  CreateCartMapper,
  UpdateCartMapper
} from '../mappers/cart.mappers';
import {
  CreateInventoryMapper,
  InventoryMapper,
  UpdateInventoryMapper
} from '../mappers/inventory.mappers';
import {
  CreateOrderMapper,
  OrderMapper,
  UpdateOrderMapper
} from '../mappers/order.mappers';
import { CreatePaymentMapper, PaymentMapper } from '../mappers/payment.mappers';
import {
  CreateProductMapper,
  ProductMapper,
  UpdateProductMapper
} from '../mappers/product.mappers';
import {
  CreateVariantMapper,
  UpdateVariantMapper,
  VariantMapper
} from '../mappers/variant.mappers';

// product
export type ProductMapperTypes = {
  ProductMapper: typeof Product;
  CreateProductMapper: typeof Product;
  UpdateProductMapper: typeof Product;
};
export type ProductDtoTypes = {
  ProductMapper: Schema<typeof ProductMapper.schema, SchemaValidator>;
  CreateProductMapper: Schema<
    typeof CreateProductMapper.schema,
    SchemaValidator
  >;
  UpdateProductMapper: Schema<
    typeof UpdateProductMapper.schema,
    SchemaValidator
  >;
};

// variant
export type VariantMapperTypes = {
  VariantMapper: typeof Variant;
  CreateVariantMapper: typeof Variant;
  UpdateVariantMapper: typeof Variant;
};
export type VariantDtoTypes = {
  VariantMapper: Schema<typeof VariantMapper.schema, SchemaValidator>;
  CreateVariantMapper: Schema<
    typeof CreateVariantMapper.schema,
    SchemaValidator
  >;
  UpdateVariantMapper: Schema<
    typeof UpdateVariantMapper.schema,
    SchemaValidator
  >;
};

// inventory
export type InventoryMapperTypes = {
  InventoryMapper: typeof Inventory;
  CreateInventoryMapper: typeof Inventory;
  UpdateInventoryMapper: typeof Inventory;
};
export type InventoryDtoTypes = {
  InventoryMapper: Schema<typeof InventoryMapper.schema, SchemaValidator>;
  CreateInventoryMapper: Schema<
    typeof CreateInventoryMapper.schema,
    SchemaValidator
  >;
  UpdateInventoryMapper: Schema<
    typeof UpdateInventoryMapper.schema,
    SchemaValidator
  >;
};

// cart
export type CartMapperTypes = {
  CartMapper: typeof Cart;
  CreateCartMapper: typeof Cart;
  UpdateCartMapper: typeof Cart;
};
export type CartDtoTypes = {
  CartMapper: Schema<typeof CartMapper.schema, SchemaValidator>;
  CreateCartMapper: Schema<typeof CreateCartMapper.schema, SchemaValidator>;
  UpdateCartMapper: Schema<typeof UpdateCartMapper.schema, SchemaValidator>;
};

// order
export type OrderMapperTypes = {
  OrderMapper: typeof Order;
  CreateOrderMapper: typeof Order;
  UpdateOrderMapper: typeof Order;
};
export type OrderDtoTypes = {
  OrderMapper: Schema<typeof OrderMapper.schema, SchemaValidator>;
  CreateOrderMapper: Schema<typeof CreateOrderMapper.schema, SchemaValidator>;
  UpdateOrderMapper: Schema<typeof UpdateOrderMapper.schema, SchemaValidator>;
};

// payment (no update mapper — confirm/fail are provider-driven, not user-editable)
export type PaymentMapperTypes = {
  PaymentMapper: typeof Payment;
  CreatePaymentMapper: typeof Payment;
};
export type PaymentDtoTypes = {
  PaymentMapper: Schema<typeof PaymentMapper.schema, SchemaValidator>;
  CreatePaymentMapper: Schema<typeof CreatePaymentMapper.schema, SchemaValidator>;
};


// TS2883 workaround. The dependency container infers through the Base*Service
// generics, and without at least one of these mapper types nameable here, tsc
// refuses to emit its declaration ("inferred type cannot be named... not
// portable"). Empirically one anchor is enough — the compiler then inlines the
// rest as import("...") in the emitted .d.ts — but all three are re-exported
// so the list is obvious rather than looking arbitrary. New entities do not
// strictly need adding here; if TS2883 reappears after a compiler upgrade,
// this block is the place to look.
export type {
  InventoryMappers,
  OrderMappers,
  ProductMappers,
  VariantMappers
} from '@forklaunch/implementation-ecommerce-base/types';

// Same TS2883 anchor requirement as above, but for the two payment-provider
// packages: their StripePaymentMappers/PaypalPaymentMappers types are what
// the dependency container infers through StripePaymentService/
// PaypalPaymentService's constructor generics.
export type { StripePaymentMappers } from '@forklaunch/implementation-ecommerce-stripe/types';
export type { PaypalPaymentMappers } from '@forklaunch/implementation-ecommerce-paypal/types';
