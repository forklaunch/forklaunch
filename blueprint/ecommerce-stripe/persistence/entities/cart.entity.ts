import { sqlBaseProperties } from '../../../core/persistence/sql.base.properties';
import { CartItemDto } from '@forklaunch/interfaces-ecommerce/types';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Cart = defineComplianceEntity({
  name: 'Cart',
  properties: {
    ...sqlBaseProperties,
    // Reference id only — no customer PII lives on Cart itself, matching
    // billing's CheckoutSession.customerId precedent.
    customerId: fp.string().nullable().compliance('none'),
    status: fp.string().compliance('none'),
    items: fp.json<CartItemDto[]>().compliance('none')
  }
});
