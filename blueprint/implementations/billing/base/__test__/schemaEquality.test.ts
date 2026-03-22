import { isTrue } from '@forklaunch/common';
import {
  BillingPortalDto,
  CheckoutSessionDto,
  CreateBillingPortalDto,
  CreateCheckoutSessionDto,
  CreatePaymentLinkDto,
  CreatePlanDto,
  CreateSubscriptionDto,
  PaymentLinkDto,
  PlanDto,
  SubscriptionDto,
  UpdateBillingPortalDto,
  UpdateCheckoutSessionDto,
  UpdatePaymentLinkDto,
  UpdatePlanDto,
  UpdateSubscriptionDto
} from '@forklaunch/interfaces-billing/types';
import { DummyEnum, testSchemaEquality } from '@forklaunch/internal';
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

const zodUpdateBillingPortalSchema = ZodUpdateBillingPortalSchema({
  uuidId: false
});
const typeboxUpdateBillingPortalSchema = TypeboxUpdateBillingPortalSchema({
  uuidId: false
});
const zodBillingPortalSchema = ZodBillingPortalSchema({ uuidId: false });
const typeboxBillingPortalSchema = TypeboxBillingPortalSchema({
  uuidId: false
});

const zodCreateCheckoutSessionSchema = ZodCreateCheckoutSessionSchema(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxCreateCheckoutSessionSchema = TypeboxCreateCheckoutSessionSchema(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const zodUpdateCheckoutSessionSchema = ZodUpdateCheckoutSessionSchema({
  uuidId: false
})(DummyEnum, DummyEnum, DummyEnum);
const typeboxUpdateCheckoutSessionSchema = TypeboxUpdateCheckoutSessionSchema({
  uuidId: false
})(DummyEnum, DummyEnum, DummyEnum);
const zodCheckoutSessionSchema = ZodCheckoutSessionSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxCheckoutSessionSchema = TypeboxCheckoutSessionSchema({
  uuidId: false
})(DummyEnum, DummyEnum, DummyEnum);

const zodCreatePaymentLinkSchema = ZodCreatePaymentLinkSchema(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxCreatePaymentLinkSchema = TypeboxCreatePaymentLinkSchema(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const zodUpdatePaymentLinkSchema = ZodUpdatePaymentLinkSchema({
  uuidId: false
})(DummyEnum, DummyEnum, DummyEnum);
const typeboxUpdatePaymentLinkSchema = TypeboxUpdatePaymentLinkSchema({
  uuidId: false
})(DummyEnum, DummyEnum, DummyEnum);
const zodPaymentLinkSchema = ZodPaymentLinkSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxPaymentLinkSchema = TypeboxPaymentLinkSchema({
  uuidId: false
})(DummyEnum, DummyEnum, DummyEnum);

const zodCreatePlanSchema = ZodCreatePlanSchema(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxCreatePlanSchema = TypeboxCreatePlanSchema(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const zodUpdatePlanSchema = ZodUpdatePlanSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxUpdatePlanSchema = TypeboxUpdatePlanSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const zodPlanSchema = ZodPlanSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum,
  DummyEnum
);
const typeboxPlanSchema = TypeboxPlanSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum,
  DummyEnum
);

const zodCreateSubscriptionSchema = ZodCreateSubscriptionSchema(
  DummyEnum,
  DummyEnum
);
const typeboxCreateSubscriptionSchema = TypeboxCreateSubscriptionSchema(
  DummyEnum,
  DummyEnum
);
const zodUpdateSubscriptionSchema = ZodUpdateSubscriptionSchema({
  uuidId: false
})(DummyEnum, DummyEnum);
const typeboxUpdateSubscriptionSchema = TypeboxUpdateSubscriptionSchema({
  uuidId: false
})(DummyEnum, DummyEnum);
const zodSubscriptionSchema = ZodSubscriptionSchema({ uuidId: false })(
  DummyEnum,
  DummyEnum
);
const typeboxSubscriptionSchema = TypeboxSubscriptionSchema({
  uuidId: false
})(DummyEnum, DummyEnum);

describe('schema equality', () => {
  it('should be equal for billing portal', () => {
    expect(
      isTrue(
        testSchemaEquality<CreateBillingPortalDto>()(
          ZodCreateBillingPortalSchema,
          TypeboxCreateBillingPortalSchema,
          {
            customerId: 'test',
            uri: 'https://example.com',
            expiresAt: new Date()
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UpdateBillingPortalDto>()(
          zodUpdateBillingPortalSchema,
          typeboxUpdateBillingPortalSchema,
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
        testSchemaEquality<BillingPortalDto>()(
          zodBillingPortalSchema,
          typeboxBillingPortalSchema,
          {
            id: 'test',
            customerId: 'test',
            uri: 'https://example.com',
            expiresAt: new Date(),
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();
  });

  it('should be equal for checkout session', () => {
    expect(
      isTrue(
        testSchemaEquality<
          CreateCheckoutSessionDto<
            typeof DummyEnum,
            typeof DummyEnum,
            typeof DummyEnum
          >
        >()(
          zodCreateCheckoutSessionSchema,
          typeboxCreateCheckoutSessionSchema,
          {
            customerId: 'test',
            paymentMethods: [DummyEnum.A, DummyEnum.B],
            currency: DummyEnum.A,
            uri: 'https://example.com',
            successRedirectUri: 'https://example.com',
            cancelRedirectUri: 'https://example.com',
            expiresAt: new Date(),
            status: DummyEnum.A,
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          UpdateCheckoutSessionDto<
            typeof DummyEnum,
            typeof DummyEnum,
            typeof DummyEnum
          >
        >()(
          zodUpdateCheckoutSessionSchema,
          typeboxUpdateCheckoutSessionSchema,
          {
            id: 'test',
            customerId: 'test',
            paymentMethods: [DummyEnum.A, DummyEnum.B],
            successRedirectUri: 'https://example.com',
            cancelRedirectUri: 'https://example.com',
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          CheckoutSessionDto<
            typeof DummyEnum,
            typeof DummyEnum,
            typeof DummyEnum
          >
        >()(zodCheckoutSessionSchema, typeboxCheckoutSessionSchema, {
          id: 'test',
          customerId: 'test',
          paymentMethods: [DummyEnum.A, DummyEnum.B],
          currency: DummyEnum.A,
          uri: 'https://example.com',
          successRedirectUri: 'https://example.com',
          cancelRedirectUri: 'https://example.com',
          expiresAt: new Date(),
          status: DummyEnum.A,
          providerFields: {
            test: 'test'
          }
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for payment link', () => {
    expect(
      isTrue(
        testSchemaEquality<
          CreatePaymentLinkDto<
            typeof DummyEnum,
            typeof DummyEnum,
            typeof DummyEnum
          >
        >()(zodCreatePaymentLinkSchema, typeboxCreatePaymentLinkSchema, {
          amount: 100,
          currency: DummyEnum.A,
          paymentMethods: [DummyEnum.A, DummyEnum.B],
          status: DummyEnum.A,
          providerFields: {
            test: 'test'
          }
        })
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          UpdatePaymentLinkDto<
            typeof DummyEnum,
            typeof DummyEnum,
            typeof DummyEnum
          >
        >()(zodUpdatePaymentLinkSchema, typeboxUpdatePaymentLinkSchema, {
          id: 'test',
          amount: 100,
          currency: DummyEnum.A,
          paymentMethods: [DummyEnum.A, DummyEnum.B],
          providerFields: {
            test: 'test'
          }
        })
      )
    );

    expect(
      isTrue(
        testSchemaEquality<
          PaymentLinkDto<typeof DummyEnum, typeof DummyEnum, typeof DummyEnum>
        >()(zodPaymentLinkSchema, typeboxPaymentLinkSchema, {
          id: 'test',
          amount: 100,
          currency: DummyEnum.A,
          paymentMethods: [DummyEnum.A, DummyEnum.B],
          status: DummyEnum.A,
          providerFields: {
            test: 'test'
          }
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for plan', () => {
    expect(
      isTrue(
        testSchemaEquality<
          CreatePlanDto<typeof DummyEnum, typeof DummyEnum, typeof DummyEnum>
        >()(zodCreatePlanSchema, typeboxCreatePlanSchema, {
          name: 'test',
          price: 100,
          cadence: DummyEnum.A,
          features: [DummyEnum.A, DummyEnum.B],
          currency: DummyEnum.A,
          externalId: 'test',
          active: true,
          description: 'test',
          providerFields: {
            test: 'test'
          },
          billingProvider: DummyEnum.A
        })
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          UpdatePlanDto<typeof DummyEnum, typeof DummyEnum, typeof DummyEnum>
        >()(zodUpdatePlanSchema, typeboxUpdatePlanSchema, {
          id: 'test',
          name: 'test',
          price: 100,
          cadence: DummyEnum.A,
          currency: DummyEnum.A,
          features: [DummyEnum.A, DummyEnum.B],
          externalId: 'test',
          active: true,
          description: 'test',
          providerFields: {
            test: 'test'
          }
        })
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          PlanDto<typeof DummyEnum, typeof DummyEnum, typeof DummyEnum>
        >()(zodPlanSchema, typeboxPlanSchema, {
          id: 'test',
          name: 'test',
          price: 100,
          cadence: DummyEnum.A,
          currency: DummyEnum.A,
          features: [DummyEnum.A, DummyEnum.B],
          externalId: 'test',
          active: true,
          description: 'test',
          providerFields: {
            test: 'test'
          },
          billingProvider: DummyEnum.A
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for subscription', () => {
    expect(
      isTrue(
        testSchemaEquality<
          CreateSubscriptionDto<typeof DummyEnum, typeof DummyEnum>
        >()(zodCreateSubscriptionSchema, typeboxCreateSubscriptionSchema, {
          partyId: 'test',
          partyType: DummyEnum.A,
          productId: 'test',
          status: DummyEnum.A,
          active: true,
          externalId: 'test',
          startDate: new Date(),
          endDate: new Date(),
          description: 'test',
          providerFields: {
            test: 'test'
          },
          billingProvider: DummyEnum.A
        })
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          UpdateSubscriptionDto<typeof DummyEnum, typeof DummyEnum>
        >()(zodUpdateSubscriptionSchema, typeboxUpdateSubscriptionSchema, {
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
          providerFields: {
            test: 'test'
          },
          billingProvider: DummyEnum.A
        })
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<
          SubscriptionDto<typeof DummyEnum, typeof DummyEnum>
        >()(zodSubscriptionSchema, typeboxSubscriptionSchema, {
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
          providerFields: {
            test: 'test'
          },
          billingProvider: DummyEnum.A
        })
      )
    ).toBeTruthy();
  });
});
