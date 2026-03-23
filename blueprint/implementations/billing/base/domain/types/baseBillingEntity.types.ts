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
    '~entity': (typeof BillingPortal)['~entity'];
  };
  CreateBillingPortalMapper: {
    '~entity': (typeof BillingPortal)['~entity'];
  };
  UpdateBillingPortalMapper: {
    '~entity': (typeof BillingPortal)['~entity'];
  };
};

// checkout session entity types
export type BaseCheckoutSessionEntities<
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum
> = {
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

// payment link entity types
export type BasePaymentLinkEntities<
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum
> = {
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

// plan entity types
export type BasePlanEntities<
  PlanCadenceEnum,
  CurrencyEnum,
  BillingProviderEnum
> = {
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

// subscription entity types
export type BaseSubscriptionEntities<PartyType, BillingProviderType> = {
  SubscriptionMapper: {
    '~entity': (typeof Subscription)['~entity'] & {
      partyType: PartyType[keyof PartyType];
      billingProvider: BillingProviderType[keyof BillingProviderType];
    };
  };
  CreateSubscriptionMapper: {
    '~entity': (typeof Subscription)['~entity'] & {
      partyType: PartyType[keyof PartyType];
      billingProvider: BillingProviderType[keyof BillingProviderType];
    };
  };
  UpdateSubscriptionMapper: {
    '~entity': (typeof Subscription)['~entity'] & {
      partyType: PartyType[keyof PartyType];
      billingProvider: BillingProviderType[keyof BillingProviderType];
    };
  };
};
