import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  ComplianceDataService,
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import {
  BaseBillingPortalService,
  BaseCheckoutSessionService,
  BasePaymentLinkService,
  BasePlanService,
  BaseSubscriptionService
} from '@forklaunch/implementation-billing-base/services';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import { BillingProviderEnum } from './domain/enum/billingProvider.enum';
import { CurrencyEnum } from './domain/enum/currency.enum';
import { PartyEnum } from './domain/enum/party.enum';
import { PaymentMethodEnum } from './domain/enum/paymentMethod.enum';
import { PlanCadenceEnum } from './domain/enum/planCadence.enum';
import { StatusEnum } from './domain/enum/status.enum';
import {
  BillingPortalMapper,
  CreateBillingPortalMapper,
  UpdateBillingPortalMapper
} from './domain/mappers/billingPortal.mappers';
import {
  CheckoutSessionMapper,
  CreateCheckoutSessionMapper,
  UpdateCheckoutSessionMapper
} from './domain/mappers/checkoutSession.mappers';
import {
  CreatePaymentLinkMapper,
  PaymentLinkMapper,
  UpdatePaymentLinkMapper
} from './domain/mappers/paymentLink.mappers';
import {
  CreatePlanMapper,
  PlanMapper,
  UpdatePlanMapper
} from './domain/mappers/plan.mappers';
import {
  CreateSubscriptionMapper,
  SubscriptionMapper,
  UpdateSubscriptionMapper
} from './domain/mappers/subscription.mappers';
import {
  BillingPortalDtoTypes,
  BillingPortalMapperTypes,
  CheckoutSessionDtoTypes,
  CheckoutSessionMapperTypes,
  PaymentLinkDtoTypes,
  PaymentLinkMapperTypes,
  PlanDtoTypes,
  PlanMapperTypes,
  SubscriptionDtoTypes,
  SubscriptionMapperTypes
} from './domain/types/billingMappers.types';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'billing',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  },
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  },
  VERSION: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('VERSION') ?? 'v1'
  },
  DOCS_PATH: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('DOCS_PATH') ?? '/docs'
  },
  OTEL_SERVICE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_SERVICE_NAME')
  },
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('OTEL_LEVEL') ?? 'info'
  },
  OTEL_EXPORTER_OTLP_ENDPOINT: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_EXPORTER_OTLP_ENDPOINT')
  },
  HMAC_SECRET_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HMAC_SECRET_KEY')
  },
  JWKS_PUBLIC_KEY_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('JWKS_PUBLIC_KEY_URL')
  },
  IAM_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('IAM_URL')
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OpenTelemetryCollector, OTEL_LEVEL }) =>
      new RedisTtlCache(
        60 * 60 * 1000,
        OpenTelemetryCollector,
        {
          url: REDIS_URL
        },
        {
          enabled: true,
          level: OTEL_LEVEL || 'info'
        }
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  BillingPortalService: {
    lifetime: Lifetime.Scoped,
    type: BaseBillingPortalService<
      SchemaValidator,
      BillingPortalMapperTypes,
      BillingPortalDtoTypes
    >,
    factory: (
      { EntityManager, TtlCache, OpenTelemetryCollector },
      resolve,
      context
    ) =>
      new BaseBillingPortalService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        TtlCache,
        OpenTelemetryCollector,
        schemaValidator,
        {
          BillingPortalMapper,
          CreateBillingPortalMapper,
          UpdateBillingPortalMapper
        }
      )
  },
  CheckoutSessionService: {
    lifetime: Lifetime.Scoped,
    type: BaseCheckoutSessionService<
      SchemaValidator,
      typeof PaymentMethodEnum,
      typeof CurrencyEnum,
      typeof StatusEnum,
      CheckoutSessionMapperTypes,
      CheckoutSessionDtoTypes
    >,
    factory: (
      { EntityManager, TtlCache, OpenTelemetryCollector },
      resolve,
      context
    ) =>
      new BaseCheckoutSessionService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        TtlCache,
        OpenTelemetryCollector,
        schemaValidator,
        {
          CheckoutSessionMapper,
          CreateCheckoutSessionMapper,
          UpdateCheckoutSessionMapper
        }
      )
  },
  PaymentLinkService: {
    lifetime: Lifetime.Scoped,
    type: BasePaymentLinkService<
      SchemaValidator,
      typeof PaymentMethodEnum,
      typeof CurrencyEnum,
      typeof StatusEnum,
      PaymentLinkMapperTypes,
      PaymentLinkDtoTypes
    >,
    factory: (
      { EntityManager, TtlCache, OpenTelemetryCollector },
      resolve,
      context
    ) =>
      new BasePaymentLinkService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        TtlCache,
        OpenTelemetryCollector,
        schemaValidator,
        {
          PaymentLinkMapper,
          CreatePaymentLinkMapper,
          UpdatePaymentLinkMapper
        }
      )
  },
  PlanService: {
    lifetime: Lifetime.Scoped,
    type: BasePlanService<
      SchemaValidator,
      typeof PlanCadenceEnum,
      typeof CurrencyEnum,
      typeof BillingProviderEnum,
      PlanMapperTypes,
      PlanDtoTypes
    >,
    factory: ({ EntityManager, OpenTelemetryCollector }, resolve, context) =>
      new BasePlanService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OpenTelemetryCollector,
        schemaValidator,
        {
          PlanMapper,
          CreatePlanMapper,
          UpdatePlanMapper
        }
      )
  },
  SubscriptionService: {
    lifetime: Lifetime.Scoped,
    type: BaseSubscriptionService<
      SchemaValidator,
      typeof PartyEnum,
      typeof BillingProviderEnum,
      SubscriptionMapperTypes,
      SubscriptionDtoTypes
    >,
    factory: ({ EntityManager, OpenTelemetryCollector }, resolve, context) =>
      new BaseSubscriptionService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OpenTelemetryCollector,
        schemaValidator,
        {
          SubscriptionMapper,
          CreateSubscriptionMapper,
          UpdateSubscriptionMapper
        }
      )
  },
  ComplianceDataService: {
    lifetime: Lifetime.Singleton,
    type: ComplianceDataService,
    factory: ({ MikroORM, OpenTelemetryCollector }) =>
      new ComplianceDataService(MikroORM, OpenTelemetryCollector, {
        Subscription: 'partyId',
        CheckoutSession: 'customerId',
        PaymentLink: 'customerId',
        BillingPortal: 'customerId'
      })
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
