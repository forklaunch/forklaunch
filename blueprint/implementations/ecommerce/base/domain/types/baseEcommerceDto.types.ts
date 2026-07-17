import {
  CreateInventoryDto,
  CreateProductDto,
  CreateVariantDto,
  InventoryDto,
  ProductDto,
  UpdateInventoryDto,
  UpdateProductDto,
  UpdateVariantDto,
  VariantDto
} from '@forklaunch/interfaces-ecommerce/types';

// product dto types
export type BaseProductDtos = {
  ProductMapper: ProductDto;
  CreateProductMapper: CreateProductDto;
  UpdateProductMapper: UpdateProductDto;
};

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
