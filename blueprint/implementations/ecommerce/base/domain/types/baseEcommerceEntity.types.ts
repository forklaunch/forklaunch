import { Inventory, Variant } from '../../persistence/entities';

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

// Remaining entities' Entity aggregates are added incrementally as each PR lands.
