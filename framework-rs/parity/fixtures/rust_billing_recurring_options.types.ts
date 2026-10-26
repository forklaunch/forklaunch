import type {
  StripeCreatePlanDto,
  StripePlanDto,
  StripeUpdatePlanDto,
  StripeCreateSubscriptionDto,
  StripeSubscriptionDto,
  StripeUpdateSubscriptionDto,
  StripeCreatePaymentLinkDto
} from '../../../blueprint/implementations/billing/stripe/domain/types/stripe.dto.types';
type Assert<T extends true> = T;
type Optional<T, K extends keyof T> = {} extends Pick<T, K> ? true : false;
export type PlanCreateOptional = Assert<Optional<StripeCreatePlanDto, 'stripeFields'>>;
export type PlanUpdateOptional = Assert<Optional<StripeUpdatePlanDto, 'stripeFields'>>;
export type PlanEvidenceRequired = Assert<Optional<StripePlanDto, 'stripeFields'> extends false ? true : false>;
export type SubscriptionCreateOptional = Assert<Optional<StripeCreateSubscriptionDto<{ USER: 'user' }>, 'stripeFields'>>;
export type SubscriptionUpdateOptional = Assert<Optional<StripeUpdateSubscriptionDto<{ USER: 'user' }>, 'stripeFields'>>;
export type SubscriptionEvidenceRequired = Assert<Optional<StripeSubscriptionDto<{ USER: 'user' }>, 'stripeFields'> extends false ? true : false>;
export type PlanEmptyOptionsAllowed = Assert<{} extends NonNullable<StripeCreatePlanDto['stripeFields']> ? true : false>;
export type SubscriptionEmptyOptionsAllowed = Assert<{} extends NonNullable<StripeCreateSubscriptionDto<{ USER: 'user' }>['stripeFields']> ? true : false>;
export type PaymentLinkOptionsStayRequired = Assert<Optional<StripeCreatePaymentLinkDto<{ PENDING: 'pending' }>, 'stripeFields'> extends false ? true : false>;
