import {
  CreateInventoryDto,
  CreateVariantDto,
  InventoryDto,
  UpdateInventoryDto,
  UpdateVariantDto,
  VariantDto
} from '@forklaunch/interfaces-ecommerce/types';

// variant dto types
export type BaseVariantDtos = {
  VariantMapper: VariantDto;
  CreateVariantMapper: CreateVariantDto;
  UpdateVariantMapper: UpdateVariantDto;
};

// inventory dto types
export type BaseInventoryDtos = {
  InventoryMapper: InventoryDto;
  CreateInventoryMapper: CreateInventoryDto;
  UpdateInventoryMapper: UpdateInventoryDto;
};

// Remaining entities' Dto aggregates are added incrementally as each PR lands.
