import { LiteralSchema } from '@forklaunch/validator';
import {
  array,
  date,
  enum_,
  number,
  optional,
  string,
  type
} from '@forklaunch/validator/zod';
import { CurrencyEnum } from '../../enum/currency.enum';
import { PaymentMethodEnum } from '../../enum/paymentMethod.enum';
import {
  StripePaymentLink,
  StripePaymentLinkCreateParams,
  StripePaymentLinkUpdateParams
} from '../../types/stripe.dto.types';

export const CreatePaymentLinkSchema = <
  T extends Record<string, LiteralSchema>
>(
  StatusEnum: T
) => ({
  id: optional(string),
  amount: number,
  currency: enum_(CurrencyEnum),
  paymentMethods: array(enum_(PaymentMethodEnum)),
  status: enum_(StatusEnum),
  stripeFields: type<StripePaymentLinkCreateParams>()
});

export const UpdatePaymentLinkSchema = <
  T extends Record<string, LiteralSchema>
>(
  StatusEnum: T
) => ({
  id: string,
  amount: optional(number),
  currency: optional(enum_(CurrencyEnum)),
  paymentMethods: optional(array(enum_(PaymentMethodEnum))),
  status: optional(enum_(StatusEnum)),
  stripeFields: optional(type<StripePaymentLinkUpdateParams>())
});

export const PaymentLinkSchema = <T extends Record<string, LiteralSchema>>(
  StatusEnum: T
) => ({
  id: string,
  amount: number,
  currency: enum_(CurrencyEnum),
  paymentMethods: array(enum_(PaymentMethodEnum)),
  status: enum_(StatusEnum),
  stripeFields: type<StripePaymentLink>(),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const StripePaymentLinkServiceSchemas = {
  CreatePaymentLinkSchema,
  UpdatePaymentLinkSchema,
  PaymentLinkSchema
};
