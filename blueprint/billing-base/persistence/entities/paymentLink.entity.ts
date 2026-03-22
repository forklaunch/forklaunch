import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../../domain/enum/paymentMethod.enum';
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
    providerFields: p.json<unknown>().nullable()
  }
});
