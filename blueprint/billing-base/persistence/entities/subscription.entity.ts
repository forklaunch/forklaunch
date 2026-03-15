import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { BillingProviderEnum } from '../../domain/enum/billingProvider.enum';
import { PartyEnum } from '../../domain/enum/party.enum';

export const Subscription = defineEntity({
  name: 'Subscription',
  properties: {
    ...sqlBaseProperties,
    // maybe have billing period here as well
    partyId: p.string(),
    partyType: p.enum(() => PartyEnum),
    description: p.string().nullable(),
    active: p.boolean(),
    // can make one to many, but for now, just store the id
    productId: p.string(),
    // access billing provider information pointer -- especially about entitlements, that can be grabbed later
    providerFields: p.json<unknown>().nullable(),
    externalId: p.string().unique(),
    billingProvider: p.enum(() => BillingProviderEnum).nullable(),
    startDate: p.datetime(),
    endDate: p.datetime().nullable(),
    status: p.string()
  }
});

export type ISubscription = InferEntity<typeof Subscription>;
