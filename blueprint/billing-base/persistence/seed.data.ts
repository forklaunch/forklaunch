import { BillingProviderEnum } from '../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../domain/enum/currency.enum';
import { PartyEnum } from '../domain/enum/party.enum';
import { PaymentMethodEnum } from '../domain/enum/paymentMethod.enum';
import { PlanCadenceEnum } from '../domain/enum/planCadence.enum';
import { StatusEnum } from '../domain/enum/status.enum';

//! Begin seed data
export const billingProvider = {
  billingProvider: BillingProviderEnum.STRIPE,
  externalId: '1234567890',
  providerFields: {},
  createdAt: new Date(),
  updatedAt: new Date()
};

export const paymentLink = {
  amount: 1000,
  currency: CurrencyEnum.USD,
  paymentMethods: [PaymentMethodEnum.CREDIT_CARD],
  status: StatusEnum.EXPIRED,
  providerFields: null,
  createdAt: new Date(),
  updatedAt: new Date()
};

export const plan = {
  active: true,
  name: 'Basic',
  description: 'Basic plan',
  price: 1000,
  currency: CurrencyEnum.USD,
  cadence: PlanCadenceEnum.MONTHLY,
  features: ['feature1', 'feature2'],
  providerFields: {},
  externalId: '1234567890',
  billingProvider: BillingProviderEnum.STRIPE,
  createdAt: new Date(),
  updatedAt: new Date()
};

export const checkoutSession = {
  customerId: 'test@example.com',
  paymentMethods: [PaymentMethodEnum.CREDIT_CARD],
  currency: CurrencyEnum.USD,
  uri: 'checkout/seed-data',
  successRedirectUri: 'https://example.com/success',
  cancelRedirectUri: 'https://example.com/cancel',
  expiresAt: new Date(),
  status: StatusEnum.EXPIRED,
  providerFields: null,
  createdAt: new Date(),
  updatedAt: new Date()
};

export const subscription = {
  partyId: '1234567890',
  partyType: PartyEnum.USER,
  description: 'Test subscription',
  active: true,
  productId: '1234567890',
  providerFields: {},
  externalId: '1234567890',
  billingProvider: BillingProviderEnum.STRIPE,
  startDate: new Date(),
  endDate: new Date(),
  status: 'active',
  createdAt: new Date(),
  updatedAt: new Date()
};
