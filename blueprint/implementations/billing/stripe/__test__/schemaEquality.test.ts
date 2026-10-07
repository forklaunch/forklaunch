import { isTrue } from '@forklaunch/common';
import { DummyEnum, testSchemaEquality } from '@forklaunch/internal';
import Stripe from 'stripe';
import { BillingProviderEnum } from '../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../domain/enum/paymentMethod.enum';
import { PlanCadenceEnum } from '../domain/enum/planCadence.enum';
import {
  BillingPortalSchema as TypeboxBillingPortalSchema,
  CreateBillingPortalSchema as TypeboxCreateBillingPortalSchema,
  UpdateBillingPortalSchema as TypeboxUpdateBillingPortalSchema
} from '../domain/schemas/typebox/billingPortal.schema';
import {
  CheckoutSessionSchema as TypeboxCheckoutSessionSchema,
  CreateCheckoutSessionSchema as TypeboxCreateCheckoutSessionSchema,
  UpdateCheckoutSessionSchema as TypeboxUpdateCheckoutSessionSchema
} from '../domain/schemas/typebox/checkoutSession.schema';
import {
  CreatePaymentLinkSchema as TypeboxCreatePaymentLinkSchema,
  PaymentLinkSchema as TypeboxPaymentLinkSchema,
  UpdatePaymentLinkSchema as TypeboxUpdatePaymentLinkSchema
} from '../domain/schemas/typebox/paymentLink.schema';
import {
  CreatePlanSchema as TypeboxCreatePlanSchema,
  PlanSchema as TypeboxPlanSchema,
  UpdatePlanSchema as TypeboxUpdatePlanSchema
} from '../domain/schemas/typebox/plan.schema';
import {
  CreateSubscriptionSchema as TypeboxCreateSubscriptionSchema,
  SubscriptionSchema as TypeboxSubscriptionSchema,
  UpdateSubscriptionSchema as TypeboxUpdateSubscriptionSchema
} from '../domain/schemas/typebox/subscription.schema';
import {
  BillingPortalSchema as ZodBillingPortalSchema,
  CreateBillingPortalSchema as ZodCreateBillingPortalSchema,
  UpdateBillingPortalSchema as ZodUpdateBillingPortalSchema
} from '../domain/schemas/zod/billingPortal.schema';
import {
  CheckoutSessionSchema as ZodCheckoutSessionSchema,
  CreateCheckoutSessionSchema as ZodCreateCheckoutSessionSchema,
  UpdateCheckoutSessionSchema as ZodUpdateCheckoutSessionSchema
} from '../domain/schemas/zod/checkoutSession.schema';
import {
  CreatePaymentLinkSchema as ZodCreatePaymentLinkSchema,
  PaymentLinkSchema as ZodPaymentLinkSchema,
  UpdatePaymentLinkSchema as ZodUpdatePaymentLinkSchema
} from '../domain/schemas/zod/paymentLink.schema';
import {
  CreatePlanSchema as ZodCreatePlanSchema,
  PlanSchema as ZodPlanSchema,
  UpdatePlanSchema as ZodUpdatePlanSchema
} from '../domain/schemas/zod/plan.schema';
import {
  CreateSubscriptionSchema as ZodCreateSubscriptionSchema,
  SubscriptionSchema as ZodSubscriptionSchema,
  UpdateSubscriptionSchema as ZodUpdateSubscriptionSchema
} from '../domain/schemas/zod/subscription.schema';
import {
  StripeBillingPortalDto,
  StripeCheckoutSessionDto,
  StripeCreateBillingPortalDto,
  StripeCreateCheckoutSessionDto,
  StripeCreatePaymentLinkDto,
  StripeCreatePlanDto,
  StripeCreateSubscriptionDto,
  StripePaymentLinkDto,
  StripePlanDto,
  StripeSubscriptionDto,
  StripeUpdateBillingPortalDto,
  StripeUpdateCheckoutSessionDto,
  StripeUpdatePaymentLinkDto,
  StripeUpdatePlanDto,
  StripeUpdateSubscriptionDto
} from '../domain/types/stripe.dto.types';

const zodCreateCheckoutSessionSchema =
  ZodCreateCheckoutSessionSchema(DummyEnum);
const typeboxCreateCheckoutSessionSchema =
  TypeboxCreateCheckoutSessionSchema(DummyEnum);
const zodUpdateCheckoutSessionSchema =
  ZodUpdateCheckoutSessionSchema(DummyEnum);
const typeboxUpdateCheckoutSessionSchema =
  TypeboxUpdateCheckoutSessionSchema(DummyEnum);
const zodCheckoutSessionSchema = ZodCheckoutSessionSchema(DummyEnum);
const typeboxCheckoutSessionSchema = TypeboxCheckoutSessionSchema(DummyEnum);

const zodCreatePaymentLinkSchema = ZodCreatePaymentLinkSchema(DummyEnum);
const typeboxCreatePaymentLinkSchema =
  TypeboxCreatePaymentLinkSchema(DummyEnum);
const zodUpdatePaymentLinkSchema = ZodUpdatePaymentLinkSchema(DummyEnum);
const typeboxUpdatePaymentLinkSchema =
  TypeboxUpdatePaymentLinkSchema(DummyEnum);
const zodPaymentLinkSchema = ZodPaymentLinkSchema(DummyEnum);
const typeboxPaymentLinkSchema = TypeboxPaymentLinkSchema(DummyEnum);

const zodCreateSubscriptionSchema = ZodCreateSubscriptionSchema(DummyEnum);
const typeboxCreateSubscriptionSchema =
  TypeboxCreateSubscriptionSchema(DummyEnum);
const zodUpdateSubscriptionSchema = ZodUpdateSubscriptionSchema(DummyEnum);
const typeboxUpdateSubscriptionSchema =
  TypeboxUpdateSubscriptionSchema(DummyEnum);
const zodSubscriptionSchema = ZodSubscriptionSchema(DummyEnum);
const typeboxSubscriptionSchema = TypeboxSubscriptionSchema(DummyEnum);

describe('schema equality', () => {
  it('should be equal for billing portal', () => {
    expect(
      isTrue(
        testSchemaEquality<StripeCreateBillingPortalDto>()(
          ZodCreateBillingPortalSchema,
          TypeboxCreateBillingPortalSchema,
          {
            customerId: 'test',
            uri: 'https://example.com',
            expiresAt: new Date(),
            stripeFields: {} as Stripe.BillingPortal.SessionCreateParams
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeUpdateBillingPortalDto>()(
          ZodUpdateBillingPortalSchema,
          TypeboxUpdateBillingPortalSchema,
          {
            id: 'test',
            uri: 'https://example.com',
            expiresAt: new Date()
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeBillingPortalDto>()(
          ZodBillingPortalSchema,
          TypeboxBillingPortalSchema,
          {
            id: 'test',
            customerId: 'test',
            uri: 'https://example.com',
            expiresAt: new Date(),
            stripeFields: {} as Stripe.BillingPortal.Session
          }
        )
      )
    ).toBeTruthy();
  });

  it('should be equal for checkout session', () => {
    expect(
      isTrue(
        testSchemaEquality<StripeCreateCheckoutSessionDto<typeof DummyEnum>>()(
          zodCreateCheckoutSessionSchema,
          typeboxCreateCheckoutSessionSchema,
          {
            customerId: 'test',
            paymentMethods: [PaymentMethodEnum.CARD, PaymentMethodEnum.AFFIRM],
            currency: CurrencyEnum.USD,
            successRedirectUri: 'https://example.com',
            cancelRedirectUri: 'https://example.com',
            expiresAt: new Date(),
            status: DummyEnum.A,
            uri: 'https://example.com',
            stripeFields: {}
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeUpdateCheckoutSessionDto<typeof DummyEnum>>()(
          zodUpdateCheckoutSessionSchema,
          typeboxUpdateCheckoutSessionSchema,
          {
            id: 'test',
            customerId: 'test',
            paymentMethods: [PaymentMethodEnum.CARD, PaymentMethodEnum.AFFIRM],
            successRedirectUri: 'https://example.com',
            cancelRedirectUri: 'https://example.com'
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeCheckoutSessionDto<typeof DummyEnum>>()(
          zodCheckoutSessionSchema,
          typeboxCheckoutSessionSchema,
          {
            id: 'test',
            customerId: 'test',
            paymentMethods: [PaymentMethodEnum.CARD, PaymentMethodEnum.AFFIRM],
            currency: CurrencyEnum.USD,
            successRedirectUri: 'https://example.com',
            cancelRedirectUri: 'https://example.com',
            expiresAt: new Date(),
            status: DummyEnum.A,
            uri: 'https://example.com',
            stripeFields: {} as Stripe.Checkout.Session
          }
        )
      )
    ).toBeTruthy();
  });

  it('should be equal for payment link', () => {
    expect(
      isTrue(
        testSchemaEquality<StripeCreatePaymentLinkDto<typeof DummyEnum>>()(
          zodCreatePaymentLinkSchema,
          typeboxCreatePaymentLinkSchema,
          {
            amount: 100,
            currency: CurrencyEnum.USD,
            paymentMethods: [PaymentMethodEnum.CARD, PaymentMethodEnum.AFFIRM],
            status: DummyEnum.A,
            stripeFields: {} as Stripe.PaymentLinkCreateParams
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeUpdatePaymentLinkDto<typeof DummyEnum>>()(
          zodUpdatePaymentLinkSchema,
          typeboxUpdatePaymentLinkSchema,
          {
            id: 'test',
            amount: 100,
            currency: CurrencyEnum.USD,
            stripeFields: {} as Stripe.PaymentLinkUpdateParams
          }
        )
      )
    );

    expect(
      isTrue(
        testSchemaEquality<StripePaymentLinkDto<typeof DummyEnum>>()(
          zodPaymentLinkSchema,
          typeboxPaymentLinkSchema,
          {
            id: 'test',
            amount: 100,
            currency: CurrencyEnum.USD,
            paymentMethods: [PaymentMethodEnum.CARD, PaymentMethodEnum.AFFIRM],
            status: DummyEnum.A,
            stripeFields: {} as Stripe.PaymentLink
          }
        )
      )
    ).toBeTruthy();
  });

  it('should be equal for plan', () => {
    expect(
      isTrue(
        testSchemaEquality<StripeCreatePlanDto>()(
          ZodCreatePlanSchema,
          TypeboxCreatePlanSchema,
          {
            name: 'test',
            price: 100,
            cadence: PlanCadenceEnum.ANNUALLY,
            currency: CurrencyEnum.USD,
            features: [DummyEnum.A, DummyEnum.B],
            externalId: 'test',
            stripeFields: {} as Stripe.PlanCreateParams,
            active: true,
            description: 'test',
            billingProvider: BillingProviderEnum.STRIPE
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeUpdatePlanDto>()(
          ZodUpdatePlanSchema,
          TypeboxUpdatePlanSchema,
          {
            id: 'test',
            name: 'test',
            price: 100,
            cadence: PlanCadenceEnum.ANNUALLY,
            features: [DummyEnum.A, DummyEnum.B],
            externalId: 'test',
            active: true,
            description: 'test'
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripePlanDto>()(ZodPlanSchema, TypeboxPlanSchema, {
          id: 'test',
          name: 'test',
          price: 100,
          cadence: PlanCadenceEnum.ANNUALLY,
          currency: CurrencyEnum.USD,
          features: [DummyEnum.A, DummyEnum.B],
          externalId: 'test',
          active: true,
          description: 'test',
          stripeFields: {} as Stripe.Plan
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for subscription', () => {
    expect(
      isTrue(
        testSchemaEquality<StripeCreateSubscriptionDto<typeof DummyEnum>>()(
          zodCreateSubscriptionSchema,
          typeboxCreateSubscriptionSchema,
          {
            partyId: 'test',
            partyType: DummyEnum.A,
            productId: 'test',
            status: DummyEnum.A,
            active: true,
            externalId: 'test',
            startDate: new Date(),
            endDate: new Date(),
            description: 'test',
            billingProvider: BillingProviderEnum.STRIPE,
            stripeFields: {} as Stripe.SubscriptionCreateParams
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeUpdateSubscriptionDto<typeof DummyEnum>>()(
          zodUpdateSubscriptionSchema,
          typeboxUpdateSubscriptionSchema,
          {
            id: 'test',
            partyId: 'test',
            partyType: DummyEnum.A,
            productId: 'test',
            status: DummyEnum.A,
            active: true,
            externalId: 'test',
            startDate: new Date(),
            endDate: new Date(),
            description: 'test',
            billingProvider: BillingProviderEnum.STRIPE
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<StripeSubscriptionDto<typeof DummyEnum>>()(
          zodSubscriptionSchema,
          typeboxSubscriptionSchema,
          {
            id: 'test',
            partyId: 'test',
            partyType: DummyEnum.A,
            productId: 'test',
            status: DummyEnum.A,
            active: true,
            externalId: 'test',
            startDate: new Date(),
            endDate: new Date(),
            description: 'test',
            billingProvider: BillingProviderEnum.STRIPE,
            stripeFields: {} as Stripe.Subscription
          }
        )
      )
    ).toBeTruthy();
  });
});
