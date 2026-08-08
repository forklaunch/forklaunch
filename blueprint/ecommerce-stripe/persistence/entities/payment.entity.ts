import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { PaymentStatus } from '@forklaunch/interfaces-ecommerce/types';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Payment = defineComplianceEntity({
  name: 'Payment',
  properties: {
    ...sqlBaseProperties,
    orderId: fp.string().compliance('none'),
    amountCents: fp.integer().compliance('none'),
    currency: fp.string().compliance('none'),
    status: fp.enum(() => PaymentStatus).compliance('none'),
    // Stripe PaymentIntent id — never raw card data (Stripe holds that).
    providerRef: fp.string().nullable().unique().compliance('none')
  }
});
