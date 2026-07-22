import { Cart, Inventory, Product, Variant } from '../../persistence/entities';

// product entity types
export type BaseProductEntities = {
  ProductMapper: { '~entity': (typeof Product)['~entity'] };
  CreateProductMapper: { '~entity': (typeof Product)['~entity'] };
  UpdateProductMapper: { '~entity': (typeof Product)['~entity'] };
};

// variant entity types
export type BaseVariantEntities = {
  VariantMapper: { '~entity': (typeof Variant)['~entity'] };
  CreateVariantMapper: { '~entity': (typeof Variant)['~entity'] };
  UpdateVariantMapper: { '~entity': (typeof Variant)['~entity'] };
};

// inventory entity types
export type BaseInventoryEntities = {
  InventoryMapper: { '~entity': (typeof Inventory)['~entity'] };
  CreateInventoryMapper: { '~entity': (typeof Inventory)['~entity'] };
  UpdateInventoryMapper: { '~entity': (typeof Inventory)['~entity'] };
};

// cart entity types
export type BaseCartEntities = {
  CartMapper: { '~entity': (typeof Cart)['~entity'] };
  CreateCartMapper: { '~entity': (typeof Cart)['~entity'] };
  UpdateCartMapper: { '~entity': (typeof Cart)['~entity'] };
};

// Remaining entities' Entity aggregates are added incrementally as each PR lands.
