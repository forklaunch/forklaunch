import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const BillingPortal = defineComplianceEntity({
  name: 'BillingPortal',
  properties: {
    id: fp.string().primary().compliance('none'),
    customerId: fp.string().compliance('none')
  }
});

export const CheckoutSession = defineComplianceEntity({
  name: 'CheckoutSession',
  properties: {
    id: fp.string().primary().compliance('none'),
    customerId: fp.string().compliance('none'),
    paymentMethods: fp.enum().array().compliance('none'),
    currency: fp.enum().compliance('none'),
    status: fp.enum().compliance('none')
  }
});

export const PaymentLink = defineComplianceEntity({
  name: 'PaymentLink',
  properties: {
    id: fp.string().primary().compliance('none'),
    amount: fp.double().compliance('none'),
    paymentMethods: fp.enum().array().compliance('none'),
    currency: fp.enum().compliance('none'),
    status: fp.enum().compliance('none')
  }
});

export const Plan = defineComplianceEntity({
  name: 'Plan',
  properties: {
    id: fp.string().primary().compliance('none'),
    name: fp.string().compliance('none'),
    price: fp.double().compliance('none'),
    externalId: fp.string().compliance('none'),
    cadence: fp.enum().compliance('none'),
    currency: fp.enum().compliance('none'),
    billingProvider: fp.enum().nullable().compliance('none')
  }
});

export const Subscription = defineComplianceEntity({
  name: 'Subscription',
  properties: {
    id: fp.string().primary().compliance('none'),
    partyId: fp.string().compliance('none'),
    externalId: fp.string().compliance('none'),
    partyType: fp.enum().compliance('none'),
    billingProvider: fp.enum().nullable().compliance('none'),
    active: fp.boolean().compliance('none')
  }
});
