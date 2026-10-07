import { ResolvedEntity } from '@forklaunch/core/persistence';
import {
  BillingPortal,
  CheckoutSession,
  PaymentLink,
  Plan,
  Subscription
} from '../../persistence/entities';
import {
  BillingProviderEnum,
  CurrencyEnum,
  PaymentMethodEnum,
  PlanCadenceEnum
} from '../enum';

// Billing Portal Types
export type StripeBillingPortalEntities = {
  BillingPortalMapper: {
    '~entity': ResolvedEntity<(typeof BillingPortal)['~entity']>;
  };
  CreateBillingPortalMapper: {
    '~entity': ResolvedEntity<(typeof BillingPortal)['~entity']>;
  };
  UpdateBillingPortalMapper: {
    '~entity': ResolvedEntity<(typeof BillingPortal)['~entity']>;
  };
};

// Checkout Session Types
export type StripeCheckoutSessionEntities = {
  CheckoutSessionMapper: {
    '~entity': ResolvedEntity<(typeof CheckoutSession)['~entity']>;
  };
  CreateCheckoutSessionMapper: {
    '~entity': ResolvedEntity<(typeof CheckoutSession)['~entity']>;
  };
  UpdateCheckoutSessionMapper: {
    '~entity': ResolvedEntity<(typeof CheckoutSession)['~entity']>;
  };
};

// Payment Link Types
export type StripePaymentLinkEntities = {
  PaymentLinkMapper: {
    '~entity': ResolvedEntity<(typeof PaymentLink)['~entity']>;
  };
  CreatePaymentLinkMapper: {
    '~entity': ResolvedEntity<(typeof PaymentLink)['~entity']>;
  };
  UpdatePaymentLinkMapper: {
    '~entity': ResolvedEntity<(typeof PaymentLink)['~entity']>;
  };
};

// Plan Types
export type StripePlanEntities = {
  PlanMapper: {
    '~entity': ResolvedEntity<(typeof Plan)['~entity']>;
  };
  CreatePlanMapper: {
    '~entity': ResolvedEntity<(typeof Plan)['~entity']>;
  };
  UpdatePlanMapper: {
    '~entity': ResolvedEntity<(typeof Plan)['~entity']>;
  };
};

// Subscription Types
export type StripeSubscriptionEntities = {
  SubscriptionMapper: {
    '~entity': ResolvedEntity<(typeof Subscription)['~entity']>;
  };
  CreateSubscriptionMapper: {
    '~entity': ResolvedEntity<(typeof Subscription)['~entity']>;
  };
  UpdateSubscriptionMapper: {
    '~entity': ResolvedEntity<(typeof Subscription)['~entity']>;
  };
};
