import { date, optional, string, type } from '@forklaunch/validator/zod';
import {
  StripeBillingPortalDto,
  StripeCreateBillingPortalDto,
  StripeUpdateBillingPortalDto
} from '../../types/stripe.dto.types';

export const CreateBillingPortalSchema = {
  id: optional(string),
  customerId: string,
  uri: optional(string),
  expiresAt: date,
  stripeFields: optional(type<StripeCreateBillingPortalDto['stripeFields']>())
};

export const UpdateBillingPortalSchema = {
  id: string,
  uri: optional(string),
  expiresAt: optional(date),
  stripeFields: optional(type<StripeUpdateBillingPortalDto['stripeFields']>())
};

export const BillingPortalSchema = {
  id: string,
  customerId: string,
  uri: optional(string),
  expiresAt: date,
  stripeFields: type<StripeBillingPortalDto['stripeFields']>(),
  createdAt: optional(date),
  updatedAt: optional(date)
};

export const StripeBillingPortalServiceSchemas = {
  CreateBillingPortalSchema,
  UpdateBillingPortalSchema,
  BillingPortalSchema
};
