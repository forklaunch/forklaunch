import { InferEntity, RequiredEntityData } from '@mikro-orm/core';

import { PartyEnum } from '../domain/enum/party.enum';

import {
  BillingProviderEnum,
  CurrencyEnum,
  PaymentMethodEnum,
  PlanCadenceEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import Stripe from 'stripe';
import { StatusEnum } from '../domain/enum/status.enum';
import { CheckoutSession } from './entities/checkoutSession.entity';
import { PaymentLink } from './entities/paymentLink.entity';
import { Plan } from './entities/plan.entity';
import { Subscription } from './entities/subscription.entity';

//! Begin seed data
export const paymentLink = {
  amount: 1000,
  currency: CurrencyEnum.USD,
  paymentMethods: [PaymentMethodEnum.CARD],
  status: StatusEnum.EXPIRED,
  createdAt: new Date(),
  updatedAt: new Date(),
  providerFields: {} as Stripe.PaymentLink
} satisfies RequiredEntityData<InferEntity<typeof PaymentLink>>;

export const plan = {
  active: true,
  name: 'Basic',
  description: 'Basic plan',
  price: 1000,
  currency: CurrencyEnum.USD,
  cadence: PlanCadenceEnum.MONTHLY,
  features: ['feature1', 'feature2'],
  providerFields: {} as Stripe.Product,
  externalId: '1234567890',
  billingProvider: BillingProviderEnum.STRIPE,
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Plan>>;

export const checkoutSession = {
  customerId: 'test@example.com',
  paymentMethods: [PaymentMethodEnum.CARD],
  currency: CurrencyEnum.USD,
  uri: 'checkout/seed-data',
  successRedirectUri: 'https://example.com/success',
  cancelRedirectUri: 'https://example.com/cancel',
  expiresAt: new Date(),
  status: StatusEnum.EXPIRED,
  createdAt: new Date(),
  updatedAt: new Date(),
  providerFields: {} as Stripe.Checkout.Session
} satisfies RequiredEntityData<InferEntity<typeof CheckoutSession>>;

export const subscription = {
  partyId: '1234567890',
  partyType: PartyEnum.USER,
  description: 'Test subscription',
  active: true,
  productId: '1234567890',
  providerFields: {} as Stripe.Subscription,
  externalId: '1234567890',
  billingProvider: BillingProviderEnum.STRIPE,
  startDate: new Date(),
  endDate: new Date(),
  status: 'active',
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Subscription>>;
