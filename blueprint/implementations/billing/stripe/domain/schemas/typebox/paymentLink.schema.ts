import { LiteralSchema } from '@forklaunch/validator';
import {
  array,
  date,
  enum_,
  number,
  optional,
  record,
  schemify,
  unknown,
  string,
  type
} from '@forklaunch/validator/typebox';
import { CurrencyEnum } from '../../enum/currency.enum';
import { PaymentMethodEnum } from '../../enum/paymentMethod.enum';
import {
  StripePaymentLinkCreateParams,
  StripePaymentLinkDto,
  StripeUpdatePaymentLinkDto
} from '../../types/stripe.dto.types';

// Structural provider input validation only; arbitrary nested options are retained.
// Keep the existing SDK static DTO contract; nested SDK fields are not validated here.
const paymentLinkCreateFields = {
  ...schemify({ line_items: array(record(string, unknown)) }),
  additionalProperties: true
} as unknown as ReturnType<typeof type<StripePaymentLinkCreateParams>>;

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
  stripeFields: paymentLinkCreateFields
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
  stripeFields: optional(type<StripeUpdatePaymentLinkDto<T>['stripeFields']>())
});

export const PaymentLinkSchema = <T extends Record<string, LiteralSchema>>(
  StatusEnum: T
) => ({
  id: string,
  amount: number,
  currency: enum_(CurrencyEnum),
  paymentMethods: array(enum_(PaymentMethodEnum)),
  status: enum_(StatusEnum),
  stripeFields: type<StripePaymentLinkDto<T>['stripeFields']>(),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const StripePaymentLinkServiceSchemas = {
  CreatePaymentLinkSchema,
  UpdatePaymentLinkSchema,
  PaymentLinkSchema
};
