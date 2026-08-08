import { SchemaValidator } from '../../schema';
import { Schema } from '@forklaunch/validator';
import { Inventory, Product, Variant } from '../../persistence/entities';
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

// Remaining entities' mapper/DTO types are added incrementally as each PR lands.

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
  ProductMappers,
  VariantMappers
} from '@forklaunch/implementation-ecommerce-base/types';
