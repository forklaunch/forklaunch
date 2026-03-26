import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  CurrencyEnum,
  PaymentMethodEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import Stripe from 'stripe';
import { StatusEnum } from '../../domain/enum/status.enum';

// This is to represent connection information for a billing provider
export const CheckoutSession = defineComplianceEntity({
  name: 'CheckoutSession',
  properties: {
    ...sqlBaseProperties,
    customerId: fp.string().compliance('none'),
    paymentMethods: fp
      .enum(() => PaymentMethodEnum)
      .array()
      .compliance('none'),
    currency: fp.enum(() => CurrencyEnum).compliance('none'),
    uri: fp.string().compliance('none'),
    successRedirectUri: fp.string().nullable().compliance('none'),
    cancelRedirectUri: fp.string().nullable().compliance('none'),
    expiresAt: fp.datetime().compliance('none'),
    status: fp.enum(() => StatusEnum).compliance('none'),
    providerFields: fp.json<Stripe.Checkout.Session>().compliance('none')
  }
});
