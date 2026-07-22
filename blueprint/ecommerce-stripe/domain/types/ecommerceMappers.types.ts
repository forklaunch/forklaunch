import { SchemaValidator } from '../../schema';
import { Schema } from '@forklaunch/validator';
import { Cart, Inventory, Product, Variant } from '../../persistence/entities';
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
  CreateProductMapper: Schema<typeof CreateProductMapper.schema, SchemaValidator>;
  UpdateProductMapper: Schema<typeof UpdateProductMapper.schema, SchemaValidator>;
};

// variant
export type VariantMapperTypes = {
  VariantMapper: typeof Variant;
  CreateVariantMapper: typeof Variant;
  UpdateVariantMapper: typeof Variant;
};
export type VariantDtoTypes = {
  VariantMapper: Schema<typeof VariantMapper.schema, SchemaValidator>;
  CreateVariantMapper: Schema<typeof CreateVariantMapper.schema, SchemaValidator>;
  UpdateVariantMapper: Schema<typeof UpdateVariantMapper.schema, SchemaValidator>;
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

// Remaining entities' mapper/DTO types are added incrementally as each PR lands.

// Re-exported so the emitted declaration can name these types without
// reaching into implementation-ecommerce-base's internal paths. Without this,
// tsc raises TS2883 ("inferred type cannot be named... not portable") on the
// dependency container, which infers through the Base*Service generics.
export type {
  InventoryMappers,
  ProductMappers,
  VariantMappers
} from '@forklaunch/implementation-ecommerce-base/types';
