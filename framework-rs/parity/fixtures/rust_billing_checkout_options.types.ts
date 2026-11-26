// Compile with the actual billing DTO source and Stripe SDK types.
import type {
  StripeCheckoutSessionDto,
  StripeCreateCheckoutSessionDto,
  StripeUpdateCheckoutSessionDto
} from '../../../blueprint/implementations/billing/stripe/domain/types/stripe.dto.types';
import type { StatusEnum } from '../../../blueprint/billing-stripe/domain/enum/status.enum';

type Assert<T extends true> = T;
type Optional<T, K extends keyof T> = {} extends Pick<T, K> ? true : false;
export type CreateOptionsCanBeOmitted = Assert<
  Optional<StripeCreateCheckoutSessionDto<typeof StatusEnum>, 'stripeFields'>
>;
export type UpdateOptionsStayOptional = Assert<
  Optional<StripeUpdateCheckoutSessionDto<typeof StatusEnum>, 'stripeFields'>
>;
export type ResponseEvidenceStaysRequired = Assert<
  Optional<StripeCheckoutSessionDto<typeof StatusEnum>, 'stripeFields'> extends false
    ? true
    : false
>;
