import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { BillingProviderEnum } from '@forklaunch/implementation-billing-stripe/enum';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import Stripe from 'stripe';
import { PartyEnum } from '../../domain/enum/party.enum';

export const Subscription = defineComplianceEntity({
  name: 'Subscription',
  properties: {
    ...sqlBaseProperties,
    // maybe have billing period here as well
    partyId: fp.string().compliance('none'),
    partyType: fp.enum(() => PartyEnum).compliance('none'),
    description: fp.string().nullable().compliance('none'),
    active: fp.boolean().compliance('none'),
    // can make one to many, but for now, just store the id
    productId: fp.string().compliance('none'),
    // access billing provider information pointer -- especially about entitlements, that can be grabbed later
    providerFields: fp.json<Stripe.Subscription>().compliance('none'),
    externalId: fp.string().unique().compliance('none'),
    billingProvider: fp.enum(() => BillingProviderEnum).compliance('none'),
    startDate: fp.datetime().compliance('none'),
    endDate: fp.datetime().nullable().compliance('none'),
    status: fp.string().compliance('none')
  }
});
