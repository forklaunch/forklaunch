import {
  array,
  boolean,
  date,
  enum_,
  number,
  optional,
  string,
  type
} from '@forklaunch/validator/typebox';
import { BillingProviderEnum } from '../../enum/billingProvider.enum';
import { CurrencyEnum } from '../../enum/currency.enum';
import { PlanCadenceEnum } from '../../enum/planCadence.enum';
import {
  StripeCreatePlanDto,
  StripePlanDto,
  StripeUpdatePlanDto
} from '../../types/stripe.dto.types';

export const CreatePlanSchema = {
  id: optional(string),
  name: string,
  description: optional(string),
  price: number,
  cadence: enum_(PlanCadenceEnum),
  currency: enum_(CurrencyEnum),
  features: optional(array(string)),
  stripeFields: type<StripeCreatePlanDto['stripeFields']>(),
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
  stripeFields: optional(type<StripeUpdatePlanDto['stripeFields']>()),
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
  stripeFields: type<StripePlanDto['stripeFields']>(),
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
