import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import Stripe from 'stripe';

export const BillingPortal = defineComplianceEntity({
  name: 'BillingPortal',
  properties: {
    id: fp.string().primary().compliance('none'),
    customerId: fp.string().compliance('none'),
    providerFields: fp.json<Stripe.BillingPortal.Session>().compliance('none')
  }
});

export const CheckoutSession = defineComplianceEntity({
  name: 'CheckoutSession',
  properties: {
    id: fp.string().primary().compliance('none'),
    customerId: fp.string().compliance('none'),
    paymentMethods: fp.enum().array().compliance('none'),
    currency: fp.enum().compliance('none'),
    status: fp.enum().compliance('none'),
    providerFields: fp.json<Stripe.Checkout.Session>().compliance('none')
  }
});

export const PaymentLink = defineComplianceEntity({
  name: 'PaymentLink',
  properties: {
    id: fp.string().primary().compliance('none'),
    amount: fp.double().compliance('none'),
    paymentMethods: fp.enum().array().compliance('none'),
    currency: fp.enum().compliance('none'),
    status: fp.enum().compliance('none'),
    providerFields: fp.json<Stripe.PaymentLink>().compliance('none')
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
    billingProvider: fp.enum().compliance('none'),
    providerFields: fp.json<Stripe.Product>().compliance('none')
  }
});

export const Subscription = defineComplianceEntity({
  name: 'Subscription',
  properties: {
    id: fp.string().primary().compliance('none'),
    partyId: fp.string().compliance('none'),
    externalId: fp.string().compliance('none'),
    partyType: fp.enum().compliance('none'),
    billingProvider: fp.enum().compliance('none'),
    active: fp.boolean().compliance('none'),
    providerFields: fp.json<Stripe.Subscription>().compliance('none')
  }
});

export const StripeWebhookEvent = defineComplianceEntity({
  name: 'StripeWebhookEvent',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    createdAt: fp
      .datetime()
      .onCreate(() => new Date())
      .compliance('none'),
    updatedAt: fp
      .datetime()
      .onCreate(() => new Date())
      .onUpdate(() => new Date())
      .compliance('none'),
    stripeId: fp.string().compliance('none'),
    idempotencyKey: fp.string().nullable().compliance('none'),
    eventType: fp.string().compliance('none'),
    eventData: fp.json<unknown>().compliance('none')
  }
});
