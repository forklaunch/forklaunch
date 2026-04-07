import { LiteralSchema } from '@forklaunch/validator';
import {
  boolean,
  date,
  enum_,
  optional,
  string,
  type
} from '@forklaunch/validator/zod';
import { BillingProviderEnum } from '../../enum/billingProvider.enum';
import {
  StripeSubscription,
  StripeSubscriptionCreateParams,
  StripeSubscriptionUpdateParams
} from '../../types/stripe.dto.types';

type SubscriptionOmissions = 'items' | 'customer';

export const CreateSubscriptionSchema = <
  T extends Record<string, LiteralSchema>
>(
  PartyEnum: T
) => ({
  id: optional(string),
  partyId: string,
  partyType: enum_(PartyEnum),
  productId: string,
  description: optional(string),
  active: boolean,
  externalId: string,
  startDate: date,
  endDate: optional(date),
  status: string,
  billingProvider: enum_(BillingProviderEnum),
  stripeFields:
    type<Omit<StripeSubscriptionCreateParams, SubscriptionOmissions>>()
});

export const UpdateSubscriptionSchema = <
  T extends Record<string, LiteralSchema>
>(
  PartyEnum: T
) => ({
  id: string,
  partyId: optional(string),
  partyType: optional(enum_(PartyEnum)),
  productId: optional(string),
  description: optional(string),
  active: optional(boolean),
  externalId: optional(string),
  startDate: optional(date),
  endDate: optional(date),
  status: optional(string),
  billingProvider: optional(enum_(BillingProviderEnum)),
  stripeFields: optional(
    type<Omit<StripeSubscriptionUpdateParams, SubscriptionOmissions>>()
  )
});

export const SubscriptionSchema = <T extends Record<string, LiteralSchema>>(
  PartyEnum: T
) => ({
  id: string,
  partyId: string,
  partyType: enum_(PartyEnum),
  productId: string,
  description: optional(string),
  active: boolean,
  externalId: string,
  startDate: date,
  endDate: optional(date),
  status: string,
  billingProvider: enum_(BillingProviderEnum),
  stripeFields: type<StripeSubscription>(),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const StripeSubscriptionServiceSchemas = {
  CreateSubscriptionSchema,
  UpdateSubscriptionSchema,
  SubscriptionSchema
};
