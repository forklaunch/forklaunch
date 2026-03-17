import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../../domain/enum/paymentMethod.enum';
import { StatusEnum } from '../../domain/enum/status.enum';

// This is to represent connection information for a billing provider
export const paymentLink = defineEntity({
  name: 'PaymentLink',
  properties: {
    ...sqlBaseProperties,
    amount: p.number(),
    paymentMethods: p.enum(() => PaymentMethodEnum),
    currency: p.enum(() => CurrencyEnum),
    description: p.string().nullable(),
    status: p.enum(() => StatusEnum),
    providerFields: p.json<unknown>().nullable()
  }
});

export type PaymentLink = InferEntity<typeof paymentLink>;
