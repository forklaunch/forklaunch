import { LiteralSchema } from '@forklaunch/validator';
import {
  boolean,
  date,
  enum_,
  optional,
  string,
  type
} from '@forklaunch/validator/typebox';
import { BillingProviderEnum } from '../../enum/billingProvider.enum';
import {
  StripeCreateSubscriptionDto,
  StripeSubscriptionDto,
  StripeUpdateSubscriptionDto
} from '../../types/stripe.dto.types';

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
  stripeFields: type<StripeCreateSubscriptionDto<T>['stripeFields']>()
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
  stripeFields: optional(type<StripeUpdateSubscriptionDto<T>['stripeFields']>())
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
  stripeFields: type<StripeSubscriptionDto<T>['stripeFields']>(),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const StripeSubscriptionServiceSchemas = {
  CreateSubscriptionSchema,
  UpdateSubscriptionSchema,
  SubscriptionSchema
};
