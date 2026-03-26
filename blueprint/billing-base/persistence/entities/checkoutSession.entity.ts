import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../../domain/enum/paymentMethod.enum';
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
    uri: fp.string().unique().nullable().compliance('none'),
    successRedirectUri: fp.string().nullable().compliance('none'),
    cancelRedirectUri: fp.string().nullable().compliance('none'),
    expiresAt: fp.datetime().compliance('none'),
    status: fp.enum(() => StatusEnum).compliance('none'),
    providerFields: fp.json<unknown>().nullable().compliance('none')
  }
});
