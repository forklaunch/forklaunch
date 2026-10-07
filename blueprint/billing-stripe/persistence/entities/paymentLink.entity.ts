import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  CurrencyEnum,
  PaymentMethodEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import { StripePaymentLink } from '@forklaunch/implementation-billing-stripe/types';
import { StatusEnum } from '../../domain/enum/status.enum';

// This is to represent connection information for a billing provider
export const PaymentLink = defineComplianceEntity({
  name: 'PaymentLink',
  properties: {
    ...sqlBaseProperties,
    amount: fp.double().compliance('none'),
    paymentMethods: fp
      .enum(() => PaymentMethodEnum)
      .array()
      .compliance('none'),
    currency: fp.enum(() => CurrencyEnum).compliance('none'),
    description: fp.string().nullable().compliance('none'),
    status: fp.enum(() => StatusEnum).compliance('none'),
    providerFields: fp.json<StripePaymentLink>().compliance('none')
  }
});
