import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../../domain/enum/paymentMethod.enum';
import { StatusEnum } from '../../domain/enum/status.enum';

// This is to represent connection information for a billing provider
export const CheckoutSession = defineEntity({
  name: 'CheckoutSession',
  properties: {
    ...sqlBaseProperties,
    customerId: p.string(),
    paymentMethods: p.enum(() => PaymentMethodEnum).array(),
    currency: p.enum(() => CurrencyEnum),
    uri: p.string().unique().nullable(),
    successRedirectUri: p.string().nullable(),
    cancelRedirectUri: p.string().nullable(),
    expiresAt: p.datetime(),
    status: p.enum(() => StatusEnum),
    providerFields: p.json<unknown>().nullable()
  }
});
