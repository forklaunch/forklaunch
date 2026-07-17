import {
  boolean,
  date,
  number,
  optional,
  record,
  string,
  uuid
} from '@forklaunch/validator/zod';

export const CreateVariantSchema = {
  productId: string,
  externalId: string,
  sku: optional(string),
  title: string,
  optionValues: optional(record(string, string)),
  priceCents: number,
  compareAtPriceCents: optional(number),
  requiresShipping: optional(boolean)
};

export const UpdateVariantSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  productId: optional(string),
  externalId: optional(string),
  sku: optional(string),
  title: optional(string),
  optionValues: optional(record(string, string)),
  priceCents: optional(number),
  compareAtPriceCents: optional(number),
  requiresShipping: optional(boolean)
});

export const VariantSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  productId: string,
  externalId: string,
  sku: optional(string),
  title: string,
  optionValues: optional(record(string, string)),
  priceCents: number,
  compareAtPriceCents: optional(number),
  requiresShipping: optional(boolean),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const BaseVariantServiceSchemas = (options: { uuidId: boolean }) => ({
  CreateVariantSchema,
  UpdateVariantSchema: UpdateVariantSchema(options),
  VariantSchema: VariantSchema(options)
});
