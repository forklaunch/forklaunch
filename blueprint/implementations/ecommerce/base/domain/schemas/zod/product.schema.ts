import {
  array,
  boolean,
  date,
  number,
  optional,
  string,
  uuid
} from '@forklaunch/validator/zod';

const ProductOptionSchema = {
  name: string,
  isPackQuantity: boolean,
  values: array(string)
};

const ProductImageSchema = {
  src: string,
  position: number
};

export const CreateProductSchema = {
  externalId: string,
  handle: string,
  sourceUrl: optional(string),
  title: string,
  descriptionHtml: optional(string),
  vendor: optional(string),
  productType: optional(string),
  tags: optional(array(string)),
  options: optional(array(ProductOptionSchema)),
  images: optional(array(ProductImageSchema))
};

export const UpdateProductSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  externalId: optional(string),
  handle: optional(string),
  sourceUrl: optional(string),
  title: optional(string),
  descriptionHtml: optional(string),
  vendor: optional(string),
  productType: optional(string),
  tags: optional(array(string)),
  options: optional(array(ProductOptionSchema)),
  images: optional(array(ProductImageSchema))
});

export const ProductSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  externalId: string,
  handle: string,
  sourceUrl: optional(string),
  title: string,
  descriptionHtml: optional(string),
  vendor: optional(string),
  productType: optional(string),
  tags: optional(array(string)),
  options: optional(array(ProductOptionSchema)),
  images: optional(array(ProductImageSchema)),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const BaseProductServiceSchemas = (options: { uuidId: boolean }) => ({
  CreateProductSchema,
  UpdateProductSchema: UpdateProductSchema(options),
  ProductSchema: ProductSchema(options)
});
