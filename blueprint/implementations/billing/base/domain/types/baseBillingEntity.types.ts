import { ResolvedEntity } from '@forklaunch/core/persistence';
import {
  BillingPortal,
  CheckoutSession,
  PaymentLink,
  Plan,
  Subscription
} from '../../persistence/entities';

// billing portal entity types
export type BaseBillingEntities = {
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

// checkout session entity types
export type BaseCheckoutSessionEntities = {
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

// payment link entity types
export type BasePaymentLinkEntities = {
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

// plan entity types
export type BasePlanEntities = {
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

// subscription entity types
export type BaseSubscriptionEntities = {
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
