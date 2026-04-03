import { getEnvVar } from '@forklaunch/common';
import {
  BillingProviderEnum,
  CurrencyEnum,
  PaymentMethodEnum,
  PlanCadenceEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import {
  BlueprintTestHarness,
  clearTestDatabase,
  DatabaseType,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager, MikroORM } from '@mikro-orm/core';
import dotenv from 'dotenv';
import * as path from 'path';
import { PartyEnum } from '../domain/enum/party.enum';
import { StatusEnum } from '../domain/enum/status.enum';

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
    needsRedis: true,
    customEnvVars: {
      STRIPE_API_KEY: getEnvVar('STRIPE_API_KEY'),
      STRIPE_WEBHOOK_SECRET: getEnvVar('STRIPE_WEBHOOK_SECRET')
    },
    onSetup: async () => {
      const Stripe = (await import('stripe')).default;
      const stripe = new Stripe(getEnvVar('STRIPE_API_KEY'));

      const paymentMethod = await stripe.paymentMethods.create({
        type: 'card',
        card: {
          token: 'tok_visa'
        }
      });

      const customer = await stripe.customers.create({
        email: 'test@example.com',
        name: 'Test Customer',
        payment_method: paymentMethod.id,
        invoice_settings: {
          default_payment_method: paymentMethod.id
        }
      });
      process.env.TEST_CUSTOMER_ID = customer.id;

      const product = await stripe.products.create({
        name: 'Test Product',
        description: 'A test product for plans'
      });
      process.env.TEST_PRODUCT_ID = product.id;

      const price = await stripe.prices.create({
        product: product.id,
        currency: 'usd',
        unit_amount: 1999,
        recurring: { interval: 'month' }
      });
      process.env.TEST_PLAN_ID = price.id;
    }
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export async function clearDatabase(options?: {
  orm?: MikroORM;
  redis?: TestSetupResult['redis'];
}): Promise<void> {
  await clearTestDatabase(options);
}

export const setupTestData = async (em: EntityManager) => {
  const { CheckoutSession } = await import(
    '../persistence/entities/checkoutSession.entity'
  );
  const { PaymentLink } = await import(
    '../persistence/entities/paymentLink.entity'
  );
  const { BillingPortal } = await import(
    '../persistence/entities/billingPortal.entity'
  );

  em.create(CheckoutSession, {
    id: '123e4567-e89b-12d3-a456-426614174004',
    customerId: 'cus_test_123',
    paymentMethods: [PaymentMethodEnum.CARD],
    currency: CurrencyEnum.USD,
    uri: 'https://checkout.stripe.com/c/pay/test_123',
    successRedirectUri: 'https://example.com/success',
    cancelRedirectUri: 'https://example.com/cancel',
    expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000),
    status: StatusEnum.PENDING,
    providerFields: {
      id: 'cs_test_123',
      object: 'checkout.session',
      status: 'open',
      customer: 'cus_test_123',
      payment_status: 'unpaid',
      url: 'https://checkout.stripe.com/c/pay/test_123',
      success_url: 'https://example.com/success',
      cancel_url: 'https://example.com/cancel',
      expires_at: Math.floor(Date.now() / 1000) + 24 * 60 * 60
    } as never
  });

  em.create(PaymentLink, {
    id: '123e4567-e89b-12d3-a456-426614174005',
    amount: 4999,
    paymentMethods: [PaymentMethodEnum.CARD],
    currency: CurrencyEnum.USD,
    description: 'A test payment link',
    status: StatusEnum.PENDING,
    providerFields: {
      id: 'plink_test_123',
      object: 'payment_link',
      active: true,
      url: 'https://checkout.stripe.com/c/pay/plink_test_123'
    } as never
  });

  em.create(BillingPortal, {
    id: '123e4567-e89b-12d3-a456-426614174006',
    customerId: 'cus_test_123',
    uri: 'https://example.com/billing',
    providerFields: {
      id: 'bps_test_123',
      object: 'billing_portal.session',
      customer: 'cus_test_123',
      return_url: 'https://example.com/billing',
      url: 'https://billing.stripe.com/session/bps_test_123'
    } as never,
    expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000)
  });

  await em.flush();
};

export const getMockPlanData = () => ({
  active: true,
  name: getEnvVar('TEST_PRODUCT_ID'),
  description: 'A new test plan',
  price: 1999,
  currency: CurrencyEnum.USD,
  cadence: PlanCadenceEnum.MONTHLY,
  features: ['feature1', 'feature2'],
  externalId: 'plan_new_123',
  billingProvider: BillingProviderEnum.STRIPE,
  stripeFields: {} as never
});

export const mockPlanData = getMockPlanData();

export const getMockUpdatePlanData = () => ({
  id: '123e4567-e89b-12d3-a456-426614174002',
  active: false,
  name: getEnvVar('TEST_PRODUCT_ID'),
  description: 'An updated test plan',
  price: 3999,
  currency: CurrencyEnum.USD,
  cadence: PlanCadenceEnum.ANNUALLY,
  features: ['feature1', 'feature2', 'feature3'],
  externalId: 'plan_updated_123',
  billingProvider: BillingProviderEnum.STRIPE
});

export const mockUpdatePlanData = getMockUpdatePlanData();

export const getMockSubscriptionData = () => ({
  partyId: getEnvVar('TEST_CUSTOMER_ID'),
  partyType: PartyEnum.USER,
  description: 'New subscription',
  active: true,
  productId: getEnvVar('TEST_PLAN_ID'),
  externalId: 'sub_new_123',
  billingProvider: BillingProviderEnum.STRIPE,
  startDate: new Date(),
  endDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
  status: 'active',
  stripeFields: {} as never
});

export const mockSubscriptionData = {
  get: () => getMockSubscriptionData()
};

export const getMockUpdateSubscriptionData = () => ({
  id: '123e4567-e89b-12d3-a456-426614174003',
  partyId: getEnvVar('TEST_CUSTOMER_ID'),
  partyType: 'user' as const,
  description: 'Updated subscription',
  active: false,
  productId: getEnvVar('TEST_PLAN_ID'),
  externalId: 'sub_updated_123',
  billingProvider: 'stripe' as const,
  startDate: new Date(),
  endDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
  status: 'cancelled'
});

export const mockUpdateSubscriptionData = {
  get: () => getMockUpdateSubscriptionData()
};

export const getMockCheckoutSessionData = () => ({
  customerId: getEnvVar('TEST_CUSTOMER_ID'),
  paymentMethods: [PaymentMethodEnum.CARD],
  currency: CurrencyEnum.USD,
  uri: 'https://checkout.stripe.com/c/pay/new_123',
  successRedirectUri: 'https://example.com/new-success',
  cancelRedirectUri: 'https://example.com/new-cancel',
  expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000),
  status: StatusEnum.PENDING,
  stripeFields: {
    line_items: [
      {
        price: getEnvVar('TEST_PLAN_ID'),
        quantity: 1
      }
    ]
  } as never
});

export const mockCheckoutSessionData = getMockCheckoutSessionData();

export const mockPaymentLinkData = {
  paymentMethods: [PaymentMethodEnum.CARD],
  currency: CurrencyEnum.USD,
  status: StatusEnum.PENDING,
  amount: 9999,
  description: 'Test Payment Link',
  stripeFields: {
    line_items: [
      {
        price_data: {
          currency: 'usd',
          product_data: {
            name: 'Test Product',
            description: 'Test Payment Link'
          },
          unit_amount: 9999
        },
        quantity: 1
      }
    ]
  }
};

export const getMockBillingPortalData = () => ({
  id: '123e4567-e89b-12d3-a456-426614174006',
  customerId: getEnvVar('TEST_CUSTOMER_ID'),
  returnUrl: 'https://example.com/billing',
  billingProvider: 'stripe' as const,
  expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000),
  stripeFields: {
    return_url: 'https://example.com/billing'
  }
});

export const mockBillingPortalData = getMockBillingPortalData();

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
