import { getEnvVar } from '@forklaunch/common';
import {
  BlueprintTestHarness,
  clearTestDatabase,
  DatabaseType,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager, MikroORM } from '@mikro-orm/core';
import dotenv from 'dotenv';
import Redis from 'ioredis';
import * as path from 'path';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: getEnvVar('DATABASE_TYPE') as DatabaseType,
    useMigrations: false,
    needsRedis: true
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export const clearDatabase = async (options?: {
  orm?: MikroORM;
  redis?: Redis;
}): Promise<void> => {
  await clearTestDatabase(options);
};

export const setupTestData = async (em: EntityManager) => {
  const { Plan } = await import('../persistence/entities/plan.entity');
  const { Subscription } = await import(
    '../persistence/entities/subscription.entity'
  );
  const { CheckoutSession } = await import(
    '../persistence/entities/checkoutSession.entity'
  );
  const { PaymentLink } = await import(
    '../persistence/entities/paymentLink.entity'
  );
  const { BillingPortal } = await import(
    '../persistence/entities/billingPortal.entity'
  );
  const { BillingProvider } = await import(
    '../persistence/entities/billingProvider.entity'
  );
  const { BillingProviderEnum } = await import(
    '../domain/enum/billingProvider.enum'
  );
  const { CurrencyEnum } = await import('../domain/enum/currency.enum');
  const { PlanCadenceEnum } = await import('../domain/enum/planCadence.enum');
  const { PartyEnum } = await import('../domain/enum/party.enum');
  const { StatusEnum } = await import('../domain/enum/status.enum');
  const { PaymentMethodEnum } = await import(
    '../domain/enum/paymentMethod.enum'
  );

  em.create(BillingProvider, {
    id: '123e4567-e89b-12d3-a456-426614174001',
    billingProvider: BillingProviderEnum.STRIPE,
    providerFields: { apiKey: 'test-api-key' },
    createdAt: new Date(),
    updatedAt: new Date()
  });

  em.create(Plan, {
    id: '123e4567-e89b-12d3-a456-426614174002',
    active: true,
    name: 'Test Plan',
    description: 'A test plan',
    price: 2999,
    currency: CurrencyEnum.USD,
    cadence: PlanCadenceEnum.MONTHLY,
    features: ['feature1', 'feature2'],
    externalId: 'plan_test_123',
    billingProvider: BillingProviderEnum.STRIPE,
    createdAt: new Date(),
    updatedAt: new Date(),
    providerFields: null
  });

  em.create(Subscription, {
    id: '123e4567-e89b-12d3-a456-426614174003',
    partyId: '123e4567-e89b-12d3-a456-426614174000',
    partyType: PartyEnum.USER,
    description: 'Test subscription',
    active: true,
    productId: '123e4567-e89b-12d3-a456-426614174002',
    externalId: 'sub_test_123',
    billingProvider: BillingProviderEnum.STRIPE,
    startDate: new Date(),
    status: 'active',
    createdAt: new Date(),
    updatedAt: new Date(),
    providerFields: null
  });

  em.create(CheckoutSession, {
    id: '123e4567-e89b-12d3-a456-426614174004',
    customerId: 'cus_test_123',
    paymentMethods: [PaymentMethodEnum.CREDIT_CARD],
    currency: CurrencyEnum.USD,
    uri: 'https://checkout.stripe.com/c/pay/test_123',
    successRedirectUri: 'https://example.com/success',
    cancelRedirectUri: 'https://example.com/cancel',
    expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000),
    status: StatusEnum.PENDING,
    createdAt: new Date(),
    updatedAt: new Date(),
    providerFields: null
  });

  em.create(PaymentLink, {
    id: '123e4567-e89b-12d3-a456-426614174005',
    amount: 4999,
    paymentMethods: [PaymentMethodEnum.CREDIT_CARD],
    currency: CurrencyEnum.USD,
    description: 'A test payment link',
    status: StatusEnum.PENDING,
    createdAt: new Date(),
    updatedAt: new Date(),
    providerFields: null
  });

  em.create(BillingPortal, {
    id: '123e4567-e89b-12d3-a456-426614174006',
    customerId: 'cus_test_123',
    uri: 'https://example.com/billing',
    providerFields: { apiKey: 'test-api-key' },
    expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000),
    createdAt: new Date(),
    updatedAt: new Date()
  });

  await em.flush();
};

export const mockPlanData = {
  active: true,
  name: 'New Plan',
  description: 'A new test plan',
  price: 1999,
  currency: 'USD' as const,
  cadence: 'MONTHLY' as const,
  features: ['feature1', 'feature2'],
  externalId: 'plan_new_123',
  billingProvider: 'stripe' as const
};

export const mockUpdatePlanData = {
  id: '123e4567-e89b-12d3-a456-426614174002',
  active: false,
  name: 'Updated Plan',
  description: 'An updated test plan',
  price: 3999,
  currency: 'USD' as const,
  cadence: 'ANNUALLY' as const,
  features: ['feature1', 'feature2', 'feature3'],
  externalId: 'plan_updated_123',
  billingProvider: 'stripe' as const,
  createdAt: new Date(),
  updatedAt: new Date()
};

export const mockSubscriptionData = {
  partyId: '123e4567-e89b-12d3-a456-426614174000',
  partyType: 'user' as const,
  description: 'New subscription',
  active: true,
  productId: '123e4567-e89b-12d3-a456-426614174002',
  externalId: 'sub_new_123',
  billingProvider: 'stripe' as const,
  startDate: new Date(),
  endDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
  status: 'active'
};

export const mockUpdateSubscriptionData = {
  id: '123e4567-e89b-12d3-a456-426614174003',
  partyId: '123e4567-e89b-12d3-a456-426614174000',
  partyType: 'user' as const,
  description: 'Updated subscription',
  active: false,
  productId: '123e4567-e89b-12d3-a456-426614174002',
  externalId: 'sub_updated_123',
  billingProvider: 'stripe' as const,
  startDate: new Date(),
  endDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
  status: 'cancelled',
  createdAt: new Date(),
  updatedAt: new Date()
};

export const mockCheckoutSessionData = {
  customerId: 'cus_new_123',
  paymentMethods: ['credit_card' as const, 'ach' as const],
  currency: 'EUR' as const,
  uri: 'https://checkout.stripe.com/c/pay/new_123',
  successRedirectUri: 'https://example.com/new-success',
  cancelRedirectUri: 'https://example.com/new-cancel',
  expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000),
  status: 'PENDING' as const
};

export const mockPaymentLinkData = {
  paymentMethods: ['credit_card' as const, 'paypal' as const],
  currency: 'GBP' as const,
  status: 'PENDING' as const,
  amount: 9999,
  price: 9999,
  description: 'Test Payment Link'
};

export const mockBillingPortalData = {
  id: '123e4567-e89b-12d3-a456-426614174006',
  customerId: 'cus_new_123',
  returnUrl: 'https://example.com/billing',
  billingProvider: 'stripe' as const,
  expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000)
};

export const mockPlanResponse = [
  {
    id: '123e4567-e89b-12d3-a456-426614174002',
    active: true,
    name: 'Test Plan',
    description: 'A test plan',
    price: 2999,
    currency: 'USD',
    cadence: 'MONTHLY',
    features: ['feature1', 'feature2'],
    externalId: 'plan_test_123',
    billingProvider: 'STRIPE',
    createdAt: expect.any(Date),
    updatedAt: expect.any(Date)
  }
];

export const mockSubscriptionResponse = [
  {
    id: '123e4567-e89b-12d3-a456-426614174003',
    partyId: '123e4567-e89b-12d3-a456-426614174000',
    partyType: 'USER',
    description: 'Test subscription',
    active: true,
    productId: '123e4567-e89b-12d3-a456-426614174002',
    externalId: 'sub_test_123',
    billingProvider: 'STRIPE',
    startDate: expect.any(Date),
    status: 'active',
    createdAt: expect.any(Date),
    updatedAt: expect.any(Date)
  }
];

export const mockCheckoutSessionResponse = [
  {
    id: '123e4567-e89b-12d3-a456-426614174004',
    customerId: 'cus_test_123',
    paymentMethods: ['CARD'],
    currency: 'USD',
    uri: 'https://checkout.stripe.com/c/pay/test_123',
    successRedirectUri: 'https://example.com/success',
    cancelRedirectUri: 'https://example.com/cancel',
    expiresAt: expect.any(Date),
    status: 'PENDING',
    createdAt: expect.any(Date),
    updatedAt: expect.any(Date)
  }
];

export const mockPaymentLinkResponse = [
  {
    id: '123e4567-e89b-12d3-a456-426614174005',
    name: 'Test Payment Link',
    description: 'A test payment link',
    price: 4999,
    currency: 'USD',
    active: true,
    externalId: 'plink_test_123',
    billingProvider: 'STRIPE',
    createdAt: expect.any(Date),
    updatedAt: expect.any(Date)
  }
];

export const mockBillingPortalResponse = {
  id: '123e4567-e89b-12d3-a456-426614174006',
  customerId: 'cus_test_123',
  returnUrl: 'https://example.com/billing',
  externalId: 'portal_test_123',
  billingProvider: 'STRIPE',
  createdAt: expect.any(Date),
  updatedAt: expect.any(Date)
};
