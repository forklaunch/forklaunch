import { ResolvedEntity } from '@forklaunch/core/persistence';
import {
  Cart,
  Inventory,
  Order,
  Payment,
  Product,
  Variant
} from '../../persistence/entities';

// product entity types
export type BaseProductEntities = {
  ProductMapper: { '~entity': ResolvedEntity<(typeof Product)['~entity']> };
  CreateProductMapper: {
    '~entity': ResolvedEntity<(typeof Product)['~entity']>;
  };
  UpdateProductMapper: {
    '~entity': ResolvedEntity<(typeof Product)['~entity']>;
  };
};

// variant entity types
export type BaseVariantEntities = {
  VariantMapper: { '~entity': ResolvedEntity<(typeof Variant)['~entity']> };
  CreateVariantMapper: {
    '~entity': ResolvedEntity<(typeof Variant)['~entity']>;
  };
  UpdateVariantMapper: {
    '~entity': ResolvedEntity<(typeof Variant)['~entity']>;
  };
};

// inventory entity types
export type BaseInventoryEntities = {
  InventoryMapper: { '~entity': ResolvedEntity<(typeof Inventory)['~entity']> };
  CreateInventoryMapper: {
    '~entity': ResolvedEntity<(typeof Inventory)['~entity']>;
  };
  UpdateInventoryMapper: {
    '~entity': ResolvedEntity<(typeof Inventory)['~entity']>;
  };
};

// cart entity types
export type BaseCartEntities = {
  CartMapper: { '~entity': ResolvedEntity<(typeof Cart)['~entity']> };
  CreateCartMapper: { '~entity': ResolvedEntity<(typeof Cart)['~entity']> };
  UpdateCartMapper: { '~entity': ResolvedEntity<(typeof Cart)['~entity']> };
};

// order entity types
export type BaseOrderEntities = {
  OrderMapper: { '~entity': ResolvedEntity<(typeof Order)['~entity']> };
  CreateOrderMapper: { '~entity': ResolvedEntity<(typeof Order)['~entity']> };
  UpdateOrderMapper: { '~entity': ResolvedEntity<(typeof Order)['~entity']> };
};

// payment entity types
export type BasePaymentEntities = {
  PaymentMapper: { '~entity': ResolvedEntity<(typeof Payment)['~entity']> };
  CreatePaymentMapper: {
    '~entity': ResolvedEntity<(typeof Payment)['~entity']>;
  };
};
