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
import { RedisWorkerProducer } from '@forklaunch/implementation-worker-redis/producers';
import { RedisWorkerSchemas } from '@forklaunch/implementation-worker-redis/schemas';
import { RedisWorkerOptions } from '@forklaunch/implementation-worker-redis/types';
import { OrderEventRecord } from './persistence/entities/orderEvent.entity';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
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
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
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
//! entity-specific services yet. Entity services are added incrementally,
//! one `.chain()` per PR, as each entity lands (same multi-step chain
//! pattern this file already uses above: configInjector -> environmentConfig
//! -> runtimeDependencies), so every intermediate PR stays independently
//! buildable rather than referencing services that don't exist yet.
const runtimeDependencies = environmentConfig.chain({
  /**
   * Cart's fast/temporary-state layer (ECOM-06's original design) — a
   * read-through cache in front of Postgres, which stays the source of
   * truth. 30 minutes matches a typical shopping-session/abandonment
   * window: long enough to serve an active session from cache, short
   * enough to self-evict abandoned carts rather than accumulate forever.
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
//! link per entity, added incrementally as each entity's PR lands.
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
    type: BaseProductService<SchemaValidator, ProductMapperTypes, ProductDtoTypes>,
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
    type: BaseVariantService<SchemaValidator, VariantMapperTypes, VariantDtoTypes>,
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
  /**
   * ECOM-12's event-emission boundary, actually implemented — previously
   * just a comment. Redis transport only (matches TtlCache already being
   * registered here; no new infra beyond what cart caching already needs).
   * Only the producer side is wired here — order.controller.ts is the only
   * local consumer of a token from this pair; the consumer/worker.ts side
   * is scoped to a follow-up PR.
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
