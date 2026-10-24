import type {
  StripeBillingPortalDto,
  StripeCreateBillingPortalDto,
  StripeUpdateBillingPortalDto
} from '../../../blueprint/implementations/billing/stripe/domain/types/stripe.dto.types';

type Assert<T extends true> = T;
type Optional<T, K extends keyof T> = {} extends Pick<T, K> ? true : false;
export type CreateOptionsCanBeOmitted = Assert<
  Optional<StripeCreateBillingPortalDto, 'stripeFields'>
>;
export type UpdateOptionsStayOptional = Assert<
  Optional<StripeUpdateBillingPortalDto, 'stripeFields'>
>;
export type ResponseEvidenceStaysRequired = Assert<
  Optional<StripeBillingPortalDto, 'stripeFields'> extends false ? true : false
>;
