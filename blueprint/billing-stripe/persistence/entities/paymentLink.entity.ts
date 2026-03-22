import { defineEntity, p } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  CurrencyEnum,
  PaymentMethodEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import Stripe from 'stripe';
import { StatusEnum } from '../../domain/enum/status.enum';

// This is to represent connection information for a billing provider
export const PaymentLink = defineEntity({
  name: 'PaymentLink',
  properties: {
    ...sqlBaseProperties,
    amount: p.double(),
    paymentMethods: p.enum(() => PaymentMethodEnum).array(),
    currency: p.enum(() => CurrencyEnum),
    description: p.string().nullable(),
    status: p.enum(() => StatusEnum),
    providerFields: p.json<Stripe.PaymentLink>()
  }
});
