import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from './schema';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FieldEncryptor,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import {
  BaseCartService,
  BaseInventoryService,
  BaseOrderService,
  BaseProductService,
  BaseVariantService
} from '@forklaunch/implementation-ecommerce-base/services';
import {
  PaypalClient,
  PaypalPaymentService
} from '@forklaunch/implementation-ecommerce-paypal/services';
import { StripePaymentService } from '@forklaunch/implementation-ecommerce-stripe/services';
import { RedisWorkerConsumer } from '@forklaunch/implementation-worker-redis/consumers';
import { RedisWorkerProducer } from '@forklaunch/implementation-worker-redis/producers';
import { RedisWorkerSchemas } from '@forklaunch/implementation-worker-redis/schemas';
import { RedisWorkerOptions } from '@forklaunch/implementation-worker-redis/types';
import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { OrderEventRecord } from './persistence/entities/orderEvent.entity';
import { StripeTaxService } from './domain/services/tax.service';
import { FlatRateShippingService } from './domain/services/shipping.service';
import { WebhookEventService } from './domain/services/webhookEvent.service';
import { OrderCartLookupService } from './domain/services/orderCartLookup.service';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import Stripe from 'stripe';
import {
  CartMapper,
  CreateCartMapper,
  UpdateCartMapper
} from './domain/mappers/cart.mappers';
import {
  CreateInventoryMapper,
  InventoryMapper,
  UpdateInventoryMapper
} from './domain/mappers/inventory.mappers';
import {
  CreateOrderMapper,
  OrderMapper,
  UpdateOrderMapper
} from './domain/mappers/order.mappers';
import {
  CreatePaymentMapper,
  PaymentMapper
} from './domain/mappers/payment.mappers';
import {
  CreateProductMapper,
  ProductMapper,
  UpdateProductMapper
} from './domain/mappers/product.mappers';
import {
  CreateVariantMapper,
  UpdateVariantMapper,
  VariantMapper
} from './domain/mappers/variant.mappers';
import {
  CartDtoTypes,
  CartMapperTypes,
  InventoryDtoTypes,
  InventoryMapperTypes,
  OrderDtoTypes,
  OrderMapperTypes,
  PaymentDtoTypes,
  PaymentMapperTypes,
  ProductDtoTypes,
  ProductMapperTypes,
  VariantDtoTypes,
  VariantMapperTypes
} from './domain/types/ecommerceMappers.types';
import mikroOrmOptionsConfig from './mikro-orm.config';

const RedisWorkerOptionsSchema = RedisWorkerSchemas({
  validator: schemaValidator
});

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'ecommerce',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
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
  STRIPE_API_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('STRIPE_API_KEY')
  },
  /** Stripe Connect (platform) mode — optional. When set, this deploy's
   *  charges are DIRECT charges on the merchant's connected account
   *  (acct_...): the merchant is merchant-of-record and settles the funds
   *  themselves. When unset, bring-your-own-key mode is unchanged: the
   *  STRIPE_API_KEY's own account is the merchant. */
  STRIPE_CONNECTED_ACCOUNT_ID: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('STRIPE_CONNECTED_ACCOUNT_ID') ?? undefined
  },
  /** Platform fee in basis points, applied only in Connect mode. Defaults
   *  to 0 — the launch business model takes NO per-transaction markup
   *  ("payments flat at launch", Guild deck); this is a dial for a future
   *  deliberate decision, not a revenue assumption. */
  STRIPE_PLATFORM_FEE_BPS: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('STRIPE_PLATFORM_FEE_BPS') ?? undefined
  },
  /** Stripe's webhook signing secret (`whsec_...`) — used by
   *  stripe.webhooks.constructEvent to verify `stripe-signature`, never to
   *  call the Stripe API itself (that's STRIPE_API_KEY). Already
   *  anticipated by __test__/test-utils.ts's customEnvVars, ahead of this
   *  PR actually consuming it. */
  STRIPE_WEBHOOK_SECRET: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('STRIPE_WEBHOOK_SECRET')
  },
  /** The webhook's own id, assigned by PayPal when the webhook URL is
   *  registered in the developer dashboard — required by PayPal's
   *  verify-webhook-signature API alongside the per-request transmission
   *  headers. */
  PAYPAL_WEBHOOK_ID: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('PAYPAL_WEBHOOK_ID')
  },
  HMAC_SECRET_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HMAC_SECRET_KEY')
  },
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
  },
  PAYPAL_CLIENT_ID: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('PAYPAL_CLIENT_ID')
  },
  PAYPAL_CLIENT_SECRET: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('PAYPAL_CLIENT_SECRET')
  },
  PAYPAL_BASE_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('PAYPAL_BASE_URL')
  },
  REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  },
  ORDER_EVENT_QUEUE: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ORDER_EVENT_QUEUE')
  }
});

//! defines the runtime dependencies for the application — pure infra, no
//! entity-specific services. Same multi-step chain pattern this file uses
//! above: configInjector -> environmentConfig -> runtimeDependencies.
const runtimeDependencies = environmentConfig.chain({
  StripeClient: {
    lifetime: Lifetime.Singleton,
    type: Stripe,
    factory: ({ STRIPE_API_KEY }) => new Stripe(STRIPE_API_KEY)
  },
  PaypalClient: {
    lifetime: Lifetime.Singleton,
    type: PaypalClient,
    factory: ({ PAYPAL_CLIENT_ID, PAYPAL_CLIENT_SECRET, PAYPAL_BASE_URL }) =>
      new PaypalClient({
        clientId: PAYPAL_CLIENT_ID,
        clientSecret: PAYPAL_CLIENT_SECRET,
        baseUrl: PAYPAL_BASE_URL
      })
  },
  TaxService: {
    lifetime: Lifetime.Singleton,
    type: StripeTaxService,
    factory: ({ StripeClient, OtelCollector }) =>
      new StripeTaxService(StripeClient, OtelCollector)
  },
  ShippingService: {
    lifetime: Lifetime.Singleton,
    type: FlatRateShippingService,
    factory: () => new FlatRateShippingService()
  },
  /**
   * Cart's fast/temporary-state layer: a read-through cache in front of
   * Postgres, which stays the source of truth. 30 minutes matches a typical
   * shopping-session/abandonment window: long enough to serve an active
   * session from cache, short enough to self-evict abandoned carts rather
   * than accumulate forever.
   */
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OtelCollector, OTEL_LEVEL, ENCRYPTION_KEY }) =>
      new RedisTtlCache(
        30 * 60 * 1000,
        OtelCollector,
        { url: REDIS_URL },
        { enabled: true, level: OTEL_LEVEL || 'info' },
        { encryptor: new FieldEncryptor(ENCRYPTION_KEY) }
      )
  },
  Orm: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: (
      { Orm },
      context: { entityManagerOptions?: ForkOptions; tenantId?: string }
    ) =>
      wrapEmWithTenantContext(
        Orm.em.fork(context?.entityManagerOptions),
        context?.tenantId
      ) as EntityManager
  }
});

//! defines the service dependencies for the application — one `.chain()`
//! link per entity.
const serviceDependencies = runtimeDependencies.chain({
  CartService: {
    lifetime: Lifetime.Scoped,
    type: BaseCartService<SchemaValidator, CartMapperTypes, CartDtoTypes>,
    factory: ({ EntityManager, OtelCollector, TtlCache }, context, resolve) =>
      new BaseCartService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        { CartMapper, CreateCartMapper, UpdateCartMapper },
        { cache: TtlCache }
      )
  },
  ProductService: {
    lifetime: Lifetime.Scoped,
    type: BaseProductService<
      SchemaValidator,
      ProductMapperTypes,
      ProductDtoTypes
    >,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseProductService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        { ProductMapper, CreateProductMapper, UpdateProductMapper }
      )
  },
  VariantService: {
    lifetime: Lifetime.Scoped,
    type: BaseVariantService<
      SchemaValidator,
      VariantMapperTypes,
      VariantDtoTypes
    >,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseVariantService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        { VariantMapper, CreateVariantMapper, UpdateVariantMapper }
      )
  },
  InventoryService: {
    lifetime: Lifetime.Scoped,
    type: BaseInventoryService<
      SchemaValidator,
      InventoryMapperTypes,
      InventoryDtoTypes
    >,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseInventoryService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        { InventoryMapper, CreateInventoryMapper, UpdateInventoryMapper }
      )
  },
  OrderService: {
    lifetime: Lifetime.Scoped,
    type: BaseOrderService<SchemaValidator, OrderMapperTypes, OrderDtoTypes>,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseOrderService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        { OrderMapper, CreateOrderMapper, UpdateOrderMapper }
      )
  },
  PaymentService: {
    lifetime: Lifetime.Scoped,
    type: StripePaymentService<
      SchemaValidator,
      PaymentMapperTypes,
      PaymentDtoTypes
    >,
    factory: (
      {
        StripeClient,
        EntityManager,
        OtelCollector,
        STRIPE_CONNECTED_ACCOUNT_ID,
        STRIPE_PLATFORM_FEE_BPS
      },
      context,
      resolve
    ) =>
      new StripePaymentService(
        StripeClient,
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        { PaymentMapper, CreatePaymentMapper },
        STRIPE_CONNECTED_ACCOUNT_ID
          ? {
              connect: {
                connectedAccountId: STRIPE_CONNECTED_ACCOUNT_ID,
                platformFeeBps: STRIPE_PLATFORM_FEE_BPS
                  ? Number(STRIPE_PLATFORM_FEE_BPS)
                  : 0
              }
            }
          : undefined
      )
  },
  /**
   * Separate token from PaymentService (Stripe, the default) rather than
   * replacing it — this is the "one socket, many providers" seam: callers
   * pick a provider per-request (see payment.controller.ts), neither
   * provider is removed to add the other.
   */
  PaypalPaymentService: {
    lifetime: Lifetime.Scoped,
    type: PaypalPaymentService<
      SchemaValidator,
      PaymentMapperTypes,
      PaymentDtoTypes
    >,
    factory: (
      { PaypalClient: paypalClientInstance, EntityManager, OtelCollector },
      context,
      resolve
    ) =>
      new PaypalPaymentService(
        paypalClientInstance,
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        // Same runtime shape as Stripe's mappers (toEntity only reads `.id`
        // off the 3rd arg) — PaypalPaymentMappers narrows the type to
        // PaypalOrder; the cast bridges the two provider-specific static
        // types over one shared runtime mapper object, same as
        // StripePaymentService does internally for its own 3rd-arg type.
        {
          PaymentMapper,
          CreatePaymentMapper
        } as unknown as ConstructorParameters<
          typeof PaypalPaymentService<
            SchemaValidator,
            PaymentMapperTypes,
            PaymentDtoTypes
          >
        >[4]
      )
  },
  /**
   * The webhook idempotency gate (the other half) — Scoped, same
   * lifetime as PaymentService/OrderService, since it holds a
   * per-request-scoped EntityManager the same way they do.
   */
  WebhookEventService: {
    lifetime: Lifetime.Scoped,
    type: WebhookEventService,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new WebhookEventService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector
      )
  },
  /**
   * Checkout-idempotency lookup — see OrderCartLookupService's
   * doc comment for why this is its own service rather than a
   * BaseOrderService method. Scoped, same lifetime/EntityManager-resolution
   * pattern as OrderService/WebhookEventService.
   */
  OrderCartLookupService: {
    lifetime: Lifetime.Scoped,
    type: OrderCartLookupService,
    factory: ({ EntityManager }, context, resolve) =>
      new OrderCartLookupService(
        context?.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager
      )
  },
  /**
   * The event-emission boundary. Redis transport only, which matches
   * TtlCache already being registered here, so it needs no infrastructure
   * beyond what cart caching already requires.
   */
  RedisWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: RedisWorkerOptionsSchema,
    value: {
      pageSize: 100,
      retries: 3,
      interval: 5000
    }
  },
  OrderEventProducer: {
    lifetime: Lifetime.Scoped,
    type: RedisWorkerProducer<OrderEventRecord, RedisWorkerOptions>,
    factory: ({ TtlCache, ORDER_EVENT_QUEUE, RedisWorkerOptions }) =>
      new RedisWorkerProducer(ORDER_EVENT_QUEUE, TtlCache, RedisWorkerOptions)
  },
  OrderEventConsumer: {
    lifetime: Lifetime.Scoped,
    // The function_([...], type<...>()) TypeBox-style signature (the exact
    // pattern sample-worker/registrations.ts uses for its own Redis
    // consumer) does not type-check here despite being byte-for-byte
    // identical — a real, unresolved TS-inference discrepancy between the
    // two files, not a typo. Falling back to an explicit type assertion on
    // the factory's return so the actual runtime behavior (verified
    // separately below) isn't blocked by it. Left as a known loose end —
    // narrowing the real cause needs more time than this pass has.
    type: null as unknown as (
      processEventsFunction: WorkerProcessFunction<OrderEventRecord>,
      failureHandler: WorkerFailureHandler<OrderEventRecord>
    ) => RedisWorkerConsumer<OrderEventRecord, RedisWorkerOptions>,
    factory:
      ({ TtlCache, ORDER_EVENT_QUEUE, RedisWorkerOptions }) =>
      (
        processEventsFunction: WorkerProcessFunction<OrderEventRecord>,
        failureHandler: WorkerFailureHandler<OrderEventRecord>
      ) =>
        new RedisWorkerConsumer(
          ORDER_EVENT_QUEUE,
          TtlCache,
          RedisWorkerOptions,
          processEventsFunction,
          failureHandler
        )
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer = (envFilePath: string) => {
  const ci = serviceDependencies.validateConfigSingletons(envFilePath);
  const tokens = serviceDependencies.tokens();
  return {
    ci,
    tokens
  };
};
