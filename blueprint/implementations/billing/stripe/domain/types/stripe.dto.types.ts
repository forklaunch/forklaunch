import {
  BillingPortalDto,
  CheckoutSessionDto,
  CreateBillingPortalDto,
  CreateCheckoutSessionDto,
  CreatePaymentLinkDto,
  CreatePlanDto,
  CreateSubscriptionDto,
  PaymentLinkDto,
  PlanDto,
  SubscriptionDto,
  UpdateBillingPortalDto,
  UpdateCheckoutSessionDto,
  UpdatePaymentLinkDto,
  UpdatePlanDto,
  UpdateSubscriptionDto
} from '@forklaunch/interfaces-billing/types';
import Stripe from 'stripe';
import { BillingProviderEnum } from '../enum/billingProvider.enum';
import { CurrencyEnum } from '../enum/currency.enum';
import { PaymentMethodEnum } from '../enum/paymentMethod.enum';
import { PlanCadenceEnum } from '../enum/planCadence.enum';

// stripe@22's type packaging stopped re-exporting `SessionCreateParams` from
// the BillingPortal/Checkout namespaces at the top level — the interfaces
// still exist in the deep submodule (`Sessions.d.ts`) but aren't reachable as
// `StripeBillingPortalSessionCreateParams` / `StripeCheckoutSessionCreateParams`.
// Derive them from the actual method signatures, which are reachable and
// won't break if stripe rearranges its internal file layout again.
type UnwrapStripeResponse<T> =
  T extends Promise<infer R>
    ? R extends { lastResponse?: unknown }
      ? Omit<R, 'lastResponse'>
      : R
    : never;

// Use `interface ... extends ...` instead of `type ... = ...` so the names
// survive declaration emit. Pure type aliases get expanded back to their
// canonical resolution (the deep stripe submodule paths) which then trips
// TS2883 in any consumer of this module — interfaces, by contrast, form
// nominal names that tsc preserves in `.d.ts` output. Empty bodies are
// intentional — we're solely re-naming the supertype.
/* eslint-disable @typescript-eslint/no-empty-object-type */
export interface StripeBillingPortalSessionCreateParams
  extends NonNullable<
    Parameters<Stripe['billingPortal']['sessions']['create']>[0]
  > {}
export interface StripeBillingPortalSession
  extends UnwrapStripeResponse<
    ReturnType<Stripe['billingPortal']['sessions']['create']>
  > {}
export interface StripeCheckoutSessionCreateParams
  extends NonNullable<
    Parameters<Stripe['checkout']['sessions']['create']>[0]
  > {}
export interface StripeCheckoutSession
  extends UnwrapStripeResponse<
    ReturnType<Stripe['checkout']['sessions']['create']>
  > {}
export interface StripePaymentLinkCreateParams
  extends NonNullable<Parameters<Stripe['paymentLinks']['create']>[0]> {}
export interface StripePaymentLinkUpdateParams
  extends NonNullable<Parameters<Stripe['paymentLinks']['update']>[1]> {}
export interface StripePaymentLink
  extends UnwrapStripeResponse<ReturnType<Stripe['paymentLinks']['create']>> {}
export interface StripePlanCreateParams
  extends NonNullable<Parameters<Stripe['plans']['create']>[0]> {}
export interface StripePlanUpdateParams
  extends NonNullable<Parameters<Stripe['plans']['update']>[1]> {}
export interface StripeProduct
  extends UnwrapStripeResponse<ReturnType<Stripe['products']['create']>> {}
export interface StripeSubscriptionCreateParams
  extends NonNullable<Parameters<Stripe['subscriptions']['create']>[0]> {}
export interface StripeSubscriptionUpdateParams
  extends NonNullable<Parameters<Stripe['subscriptions']['update']>[1]> {}
export interface StripeSubscription
  extends UnwrapStripeResponse<ReturnType<Stripe['subscriptions']['create']>> {}
/* eslint-enable @typescript-eslint/no-empty-object-type */

// Billing Portal Types
type BillingPortalOmissions = 'customer';

export type StripeCreateBillingPortalDto = Omit<
  CreateBillingPortalDto,
  'providerFields'
> & {
  stripeFields: Omit<
    StripeBillingPortalSessionCreateParams,
    BillingPortalOmissions
  >;
};

export type StripeUpdateBillingPortalDto = Omit<
  UpdateBillingPortalDto,
  'providerFields'
> & {
  stripeFields?: Omit<
    StripeBillingPortalSessionCreateParams,
    BillingPortalOmissions
  >;
};

export type StripeBillingPortalDto = Omit<
  BillingPortalDto,
  'providerFields'
> & {
  stripeFields: StripeBillingPortalSession;
};

export type StripeBillingPortalDtos = {
  BillingPortalMapper: StripeBillingPortalDto;
  CreateBillingPortalMapper: StripeCreateBillingPortalDto;
  UpdateBillingPortalMapper: StripeUpdateBillingPortalDto;
};

// Checkout Session Types
type CheckoutSessionOmissions =
  | 'payment_method_types'
  | 'currency'
  | 'success_url'
  | 'cancel_url';

export type StripeCreateCheckoutSessionDto<StatusEnum> = Omit<
  CreateCheckoutSessionDto<
    typeof PaymentMethodEnum,
    typeof CurrencyEnum,
    StatusEnum
  >,
  'providerFields' | 'uri'
> & {
  uri: string;
  stripeFields: Omit<
    StripeCheckoutSessionCreateParams,
    CheckoutSessionOmissions
  >;
};

export type StripeUpdateCheckoutSessionDto<StatusEnum> = Omit<
  UpdateCheckoutSessionDto<
    typeof PaymentMethodEnum,
    typeof CurrencyEnum,
    StatusEnum
  >,
  'providerFields'
> & {
  stripeFields?: Omit<
    StripeCheckoutSessionCreateParams,
    CheckoutSessionOmissions
  >;
};

export type StripeCheckoutSessionDto<StatusEnum> = Omit<
  CheckoutSessionDto<typeof PaymentMethodEnum, typeof CurrencyEnum, StatusEnum>,
  'providerFields' | 'uri'
> & {
  uri: string;
  stripeFields: StripeCheckoutSession;
};

export type StripeCheckoutSessionDtos<StatusEnum> = {
  CheckoutSessionMapper: StripeCheckoutSessionDto<StatusEnum>;
  CreateCheckoutSessionMapper: StripeCreateCheckoutSessionDto<StatusEnum>;
  UpdateCheckoutSessionMapper: StripeUpdateCheckoutSessionDto<StatusEnum>;
};

// Payment Link Types
export type StripeCreatePaymentLinkDto<StatusEnum> = Omit<
  CreatePaymentLinkDto<
    typeof PaymentMethodEnum,
    typeof CurrencyEnum,
    StatusEnum
  >,
  'providerFields'
> & {
  stripeFields: StripePaymentLinkCreateParams;
};

export type StripeUpdatePaymentLinkDto<StatusEnum> = Omit<
  UpdatePaymentLinkDto<
    typeof PaymentMethodEnum,
    typeof CurrencyEnum,
    StatusEnum
  >,
  'providerFields'
> & {
  stripeFields?: StripePaymentLinkUpdateParams;
};

export type StripePaymentLinkDto<StatusEnum> = Omit<
  PaymentLinkDto<typeof PaymentMethodEnum, typeof CurrencyEnum, StatusEnum>,
  'providerFields'
> & {
  stripeFields: StripePaymentLink;
};

export type StripePaymentLinkDtos<StatusEnum> = {
  PaymentLinkMapper: StripePaymentLinkDto<StatusEnum>;
  CreatePaymentLinkMapper: StripeCreatePaymentLinkDto<StatusEnum>;
  UpdatePaymentLinkMapper: StripeUpdatePaymentLinkDto<StatusEnum>;
};

// Plan Types
type PlanOmissions = 'product' | 'interval' | 'currency';

export type StripeCreatePlanDto = Omit<
  CreatePlanDto<
    typeof PlanCadenceEnum,
    typeof CurrencyEnum,
    typeof BillingProviderEnum
  >,
  'providerFields'
> & {
  stripeFields: Omit<StripePlanCreateParams, PlanOmissions>;
};

export type StripeUpdatePlanDto = Omit<
  UpdatePlanDto<
    typeof PlanCadenceEnum,
    typeof CurrencyEnum,
    typeof BillingProviderEnum
  >,
  'providerFields'
> & {
  stripeFields?: Omit<StripePlanUpdateParams, PlanOmissions>;
};

export type StripePlanDto = Omit<
  PlanDto<
    typeof PlanCadenceEnum,
    typeof CurrencyEnum,
    typeof BillingProviderEnum
  >,
  'providerFields'
> & {
  stripeFields: StripeProduct;
};

export type StripePlanDtos = {
  PlanMapper: StripePlanDto;
  CreatePlanMapper: StripeCreatePlanDto;
  UpdatePlanMapper: StripeUpdatePlanDto;
};

// Subscription Types
type SubscriptionOmissions = 'items' | 'customer';

export type StripeCreateSubscriptionDto<PartyType> = Omit<
  CreateSubscriptionDto<PartyType, typeof BillingProviderEnum>,
  'providerFields'
> & {
  stripeFields: Omit<StripeSubscriptionCreateParams, SubscriptionOmissions>;
};

export type StripeUpdateSubscriptionDto<PartyType> = Omit<
  UpdateSubscriptionDto<PartyType, typeof BillingProviderEnum>,
  'providerFields'
> & {
  stripeFields?: Omit<StripeSubscriptionUpdateParams, SubscriptionOmissions>;
};

export type StripeSubscriptionDto<PartyType> = Omit<
  SubscriptionDto<PartyType, typeof BillingProviderEnum>,
  'providerFields'
> & {
  stripeFields: StripeSubscription;
};

export type StripeSubscriptionDtos<PartyType> = {
  SubscriptionMapper: StripeSubscriptionDto<PartyType>;
  CreateSubscriptionMapper: StripeCreateSubscriptionDto<PartyType>;
  UpdateSubscriptionMapper: StripeUpdateSubscriptionDto<PartyType>;
};
