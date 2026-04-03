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
    '~entity': (typeof BillingPortal)['~entity'];
  };
  CreateBillingPortalMapper: {
    '~entity': (typeof BillingPortal)['~entity'];
  };
  UpdateBillingPortalMapper: {
    '~entity': (typeof BillingPortal)['~entity'];
  };
};

// Checkout Session Types
export type StripeCheckoutSessionEntities<StatusEnum> = {
  CheckoutSessionMapper: {
    '~entity': (typeof CheckoutSession)['~entity'] & {
      paymentMethods: PaymentMethodEnum[keyof PaymentMethodEnum][];
      currency: CurrencyEnum[keyof CurrencyEnum];
      status: StatusEnum[keyof StatusEnum];
    };
  };
  CreateCheckoutSessionMapper: {
    '~entity': (typeof CheckoutSession)['~entity'] & {
      paymentMethods: PaymentMethodEnum[keyof PaymentMethodEnum][];
      currency: CurrencyEnum[keyof CurrencyEnum];
      status: StatusEnum[keyof StatusEnum];
    };
  };
  UpdateCheckoutSessionMapper: {
    '~entity': (typeof CheckoutSession)['~entity'] & {
      paymentMethods: PaymentMethodEnum[keyof PaymentMethodEnum][];
      currency: CurrencyEnum[keyof CurrencyEnum];
      status: StatusEnum[keyof StatusEnum];
    };
  };
};

// Payment Link Types
export type StripePaymentLinkEntities<StatusEnum> = {
  PaymentLinkMapper: {
    '~entity': (typeof PaymentLink)['~entity'] & {
      paymentMethods: PaymentMethodEnum[keyof PaymentMethodEnum][];
      currency: CurrencyEnum[keyof CurrencyEnum];
      status: StatusEnum[keyof StatusEnum];
    };
  };
  CreatePaymentLinkMapper: {
    '~entity': (typeof PaymentLink)['~entity'] & {
      paymentMethods: PaymentMethodEnum[keyof PaymentMethodEnum][];
      currency: CurrencyEnum[keyof CurrencyEnum];
      status: StatusEnum[keyof StatusEnum];
    };
  };
  UpdatePaymentLinkMapper: {
    '~entity': (typeof PaymentLink)['~entity'] & {
      paymentMethods: PaymentMethodEnum[keyof PaymentMethodEnum][];
      currency: CurrencyEnum[keyof CurrencyEnum];
      status: StatusEnum[keyof StatusEnum];
    };
  };
};

// Plan Types
export type StripePlanEntities = {
  PlanMapper: {
    '~entity': (typeof Plan)['~entity'] & {
      cadence: PlanCadenceEnum[keyof PlanCadenceEnum];
      currency: CurrencyEnum[keyof CurrencyEnum];
      billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
    };
  };
  CreatePlanMapper: {
    '~entity': (typeof Plan)['~entity'] & {
      cadence: PlanCadenceEnum[keyof PlanCadenceEnum];
      currency: CurrencyEnum[keyof CurrencyEnum];
      billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
    };
  };
  UpdatePlanMapper: {
    '~entity': (typeof Plan)['~entity'] & {
      cadence: PlanCadenceEnum[keyof PlanCadenceEnum];
      currency: CurrencyEnum[keyof CurrencyEnum];
      billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
    };
  };
};

// Subscription Types
export type StripeSubscriptionEntities<PartyTypeEnum> = {
  SubscriptionMapper: {
    '~entity': (typeof Subscription)['~entity'] & {
      partyType: PartyTypeEnum[keyof PartyTypeEnum];
      billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
    };
  };
  CreateSubscriptionMapper: {
    '~entity': (typeof Subscription)['~entity'] & {
      partyType: PartyTypeEnum[keyof PartyTypeEnum];
      billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
    };
  };
  UpdateSubscriptionMapper: {
    '~entity': (typeof Subscription)['~entity'] & {
      partyType: PartyTypeEnum[keyof PartyTypeEnum];
      billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
    };
  };
};
