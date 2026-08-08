import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { ProductImage, ProductOption } from '@forklaunch/interfaces-ecommerce/types';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Product = defineComplianceEntity({
  name: 'Product',
  properties: {
    ...sqlBaseProperties,
    externalId: fp.string().unique().compliance('none'),
    handle: fp.string().unique().compliance('none'),
    sourceUrl: fp.string().nullable().compliance('none'),
    title: fp.string().compliance('none'),
    descriptionHtml: fp.string().nullable().compliance('none'),
    vendor: fp.string().nullable().compliance('none'),
    productType: fp.string().nullable().compliance('none'),
    tags: fp.string().array().nullable().compliance('none'),
    options: fp.json<ProductOption[]>().nullable().compliance('none'),
    images: fp.json<ProductImage[]>().nullable().compliance('none')
  }
});
