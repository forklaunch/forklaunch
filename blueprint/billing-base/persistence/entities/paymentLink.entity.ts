import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../../domain/enum/paymentMethod.enum';
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
    providerFields: fp.json<unknown>().nullable().compliance('none')
  }
});
