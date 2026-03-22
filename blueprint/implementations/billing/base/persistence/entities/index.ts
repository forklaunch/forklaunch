import { defineEntity, p } from '@mikro-orm/core';

export const BillingPortal = defineEntity({
  name: 'BillingPortal',
  properties: {
    id: p.string().primary(),
    customerId: p.string()
  }
});

export const CheckoutSession = defineEntity({
  name: 'CheckoutSession',
  properties: {
    id: p.string().primary(),
    customerId: p.string(),
    paymentMethods: p.enum().array(),
    currency: p.enum(),
    status: p.enum()
  }
});

export const PaymentLink = defineEntity({
  name: 'PaymentLink',
  properties: {
    id: p.string().primary(),
    amount: p.double(),
    paymentMethods: p.enum().array(),
    currency: p.enum(),
    status: p.enum()
  }
});

export const Plan = defineEntity({
  name: 'Plan',
  properties: {
    id: p.string().primary(),
    name: p.string(),
    price: p.double(),
    externalId: p.string(),
    cadence: p.enum(),
    currency: p.enum(),
    billingProvider: p.enum().nullable()
  }
});

export const Subscription = defineEntity({
  name: 'Subscription',
  properties: {
    id: p.string().primary(),
    partyId: p.string(),
    externalId: p.string(),
    partyType: p.enum(),
    billingProvider: p.enum().nullable(),
    active: p.boolean()
  }
});
