import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import {
  CartItemDto,
  OrderItemDto,
  ProductImage,
  ProductOption,
  ShippingAddressDto,
  TaxLineDto
} from '@forklaunch/interfaces-ecommerce/types';

/**
 * Base template entities — the typing scaffolds the generic base services
 * operate against. The deployable app defines the concrete entities (with
 * sqlBaseProperties + relations) and injects them via mappers.
 *
 * Catalog data (products, variants, stock) carries no PII/PCI, so every field
 * is compliance('none'). PII/PCI tagging becomes relevant with customer/order
 * entities in later stages.
 *
 * Added incrementally as each entity's PR lands.
 */
export const Product = defineComplianceEntity({
  name: 'Product',
  properties: {
    id: fp.string().primary().compliance('none'),
    externalId: fp.string().compliance('none'),
    handle: fp.string().compliance('none'),
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

export const Variant = defineComplianceEntity({
  name: 'Variant',
  properties: {
    id: fp.string().primary().compliance('none'),
    productId: fp.string().compliance('none'),
    externalId: fp.string().compliance('none'),
    sku: fp.string().nullable().compliance('none'),
    title: fp.string().compliance('none'),
    optionValues: fp.json<Record<string, string>>().nullable().compliance('none'),
    priceCents: fp.integer().compliance('none'),
    compareAtPriceCents: fp.integer().nullable().compliance('none'),
    requiresShipping: fp.boolean().compliance('none')
  }
});

export const Cart = defineComplianceEntity({
  name: 'Cart',
  properties: {
    id: fp.string().primary().compliance('none'),
    // Reference id only (no customer PII lives on this entity) — matches
    // billing's CheckoutSession.customerId precedent (compliance('none')).
    customerId: fp.string().nullable().compliance('none'),
    status: fp.string().compliance('none'),
    items: fp.json<CartItemDto[]>().compliance('none')
  }
});

export const Inventory = defineComplianceEntity({
  name: 'Inventory',
  properties: {
    id: fp.string().primary().compliance('none'),
    variantId: fp.string().compliance('none'),
    stock: fp.integer().compliance('none')
  }
});

export const Order = defineComplianceEntity({
  name: 'Order',
  properties: {
    id: fp.string().primary().compliance('none'),
    customerId: fp.string().nullable().compliance('none'),
    status: fp.string().compliance('none'),
    items: fp.json<OrderItemDto[]>().compliance('none'),
    shippingAddress: fp.json<ShippingAddressDto>().compliance('pii'),
    subtotalCents: fp.integer().compliance('none'),
    discountCents: fp.integer().compliance('none'),
    taxCents: fp.integer().compliance('none'),
    taxBreakdown: fp.json<TaxLineDto[]>().compliance('none'),
    shippingCents: fp.integer().compliance('none'),
    giftCardCents: fp.integer().compliance('none'),
    totalCents: fp.integer().compliance('none')
  }
});
