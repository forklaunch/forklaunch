import {
  date,
  number,
  optional,
  string,
  uuid
} from '@forklaunch/validator/zod';

export const CreateInventorySchema = {
  variantId: string,
  stock: number
};

export const UpdateInventorySchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  variantId: optional(string),
  stock: optional(number)
});

export const InventorySchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  variantId: string,
  stock: number,
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const AdjustStockSchema = {
  variantId: string,
  delta: number
};

export const StockCheckSchema = {
  variantId: string,
  requested: number
};

export const BaseInventoryServiceSchemas = (options: { uuidId: boolean }) => ({
  CreateInventorySchema,
  UpdateInventorySchema: UpdateInventorySchema(options),
  InventorySchema: InventorySchema(options)
});
