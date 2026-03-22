import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { BillingProviderEnum } from '@forklaunch/implementation-billing-stripe/enum';
import { defineEntity, p } from '@mikro-orm/core';
import Stripe from 'stripe';
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
    providerFields: p.json<Stripe.Subscription>(),
    externalId: p.string().unique(),
    billingProvider: p.enum(() => BillingProviderEnum),
    startDate: p.datetime(),
    endDate: p.datetime().nullable(),
    status: p.string()
  }
});
