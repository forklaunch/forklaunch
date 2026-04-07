import {
  array,
  boolean,
  date,
  enum_,
  number,
  optional,
  string,
  type
} from '@forklaunch/validator/zod';
import { BillingProviderEnum } from '../../enum/billingProvider.enum';
import { CurrencyEnum } from '../../enum/currency.enum';
import { PlanCadenceEnum } from '../../enum/planCadence.enum';
import {
  StripePlanCreateParams,
  StripePlanUpdateParams,
  StripeProduct
} from '../../types/stripe.dto.types';

type PlanOmissions = 'product' | 'interval' | 'currency';

export const CreatePlanSchema = {
  id: optional(string),
  name: string,
  description: optional(string),
  price: number,
  cadence: enum_(PlanCadenceEnum),
  currency: enum_(CurrencyEnum),
  features: optional(array(string)),
  stripeFields: type<Omit<StripePlanCreateParams, PlanOmissions>>(),
  externalId: string,
  billingProvider: enum_(BillingProviderEnum),
  active: boolean
};

export const UpdatePlanSchema = {
  id: string,
  name: optional(string),
  description: optional(string),
  price: optional(number),
  cadence: optional(enum_(PlanCadenceEnum)),
  currency: optional(enum_(CurrencyEnum)),
  features: optional(array(string)),
  stripeFields: optional(type<Omit<StripePlanUpdateParams, PlanOmissions>>()),
  externalId: optional(string),
  billingProvider: optional(enum_(BillingProviderEnum)),
  active: optional(boolean)
};

export const PlanSchema = {
  id: string,
  name: string,
  description: optional(string),
  price: number,
  cadence: enum_(PlanCadenceEnum),
  currency: enum_(CurrencyEnum),
  features: optional(array(string)),
  stripeFields: type<StripeProduct>(),
  externalId: string,
  billingProvider: enum_(BillingProviderEnum),
  active: boolean,
  createdAt: optional(date),
  updatedAt: optional(date)
};

export const StripePlanServiceSchemas = {
  CreatePlanSchema,
  UpdatePlanSchema,
  PlanSchema
};
