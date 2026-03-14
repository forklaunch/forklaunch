import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  CurrencyEnum,
  PaymentMethodEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import Stripe from 'stripe';
import { StatusEnum } from '../../domain/enum/status.enum';

// This is to represent connection information for a billing provider
export const CheckoutSession = defineEntity({
  name: 'CheckoutSession',
  properties: {
    ...sqlBaseProperties,
    customerId: p.string(),
    paymentMethods: p.enum(() => PaymentMethodEnum).array(),
    currency: p.enum(() => CurrencyEnum),
    uri: p.string(),
    successRedirectUri: p.string().nullable(),
    cancelRedirectUri: p.string().nullable(),
    expiresAt: p.datetime(),
    status: p.enum(() => StatusEnum),
    providerFields: p.json<Stripe.Checkout.Session>()
  }
});

export type ICheckoutSession = InferEntity<typeof CheckoutSession>;
