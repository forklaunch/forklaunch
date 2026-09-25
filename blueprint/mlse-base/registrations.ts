import {
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FieldEncryptor,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import {
  ComplianceDataService,
  createConfigInjector,
  getEnvVar,
  Lifetime,
  RetentionService
} from '@forklaunch/core/services';
import {
  ClinicalTrialsFetcher,
  DailyMedFetcher,
  FakeLlmProvider,
  FetchLike,
  LexicalReranker,
  LiveRetrievalService,
  OpenFdaFetcher,
  PmcOaFetcher,
  PublicCorpusProvider,
  PubMedFetcher,
  SourceFetcherRegistry
} from '@forklaunch/implementation-mlse-base/services';
import { RedisWorkerConsumer } from '@forklaunch/implementation-worker-redis/consumers';
import { RedisWorkerProducer } from '@forklaunch/implementation-worker-redis/producers';
import { RedisWorkerSchemas } from '@forklaunch/implementation-worker-redis/schemas';
import { RedisWorkerOptions } from '@forklaunch/implementation-worker-redis/types';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import { IngestionService } from './domain/services/ingestion.service';
import { SearchService } from './domain/services/search.service';
import { TopicService } from './domain/services/topic.service';
import { IngestionJob } from './domain/types/ingestionJob.types';
import mikroOrmOptionsConfig from './mikro-orm.config';

const RedisWorkerOptionsSchema = RedisWorkerSchemas({
  validator: schemaValidator
});

// Dimension used by the development AI provider. A production embedding model
// fixes its own dimension, which also fixes the vector column size, so it is
// configured explicitly rather than guessed.
const DEFAULT_EMBEDDING_DIMENSIONS = 8;

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'mlse',
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
  IAM_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('IAM_URL')
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
  MLSE_INGESTION_QUEUE: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('MLSE_INGESTION_QUEUE')
  },
  // Optional keys are empty strings when unset (the config schema needs a
  // string); consumers treat '' as "no key".
  // NCBI asks applications calling E-utilities to identify themselves with a
  // tool name and contact email, and to let each deployment supply its own
  // API key rather than sharing one.
  NCBI_TOOL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('NCBI_TOOL')
  },
  NCBI_EMAIL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('NCBI_EMAIL')
  },
  NCBI_API_KEY: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('NCBI_API_KEY') ?? ''
  },
  OPENFDA_API_KEY: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('OPENFDA_API_KEY') ?? ''
  },
  LLM_PROVIDER: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('LLM_PROVIDER') || 'fake'
  },
  // Budget for querying live sources during one search, kept well inside
  // the 10-second first-answer target.
  LIVE_RETRIEVAL_TIMEOUT_MS: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: getEnvVar('LIVE_RETRIEVAL_TIMEOUT_MS')
      ? Number(getEnvVar('LIVE_RETRIEVAL_TIMEOUT_MS'))
      : 4000
  },
  EMBEDDING_DIMENSIONS: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: getEnvVar('EMBEDDING_DIMENSIONS')
      ? Number(getEnvVar('EMBEDDING_DIMENSIONS'))
      : DEFAULT_EMBEDDING_DIMENSIONS
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
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
  /**
   * Backs both the ingestion queue and, from live retrieval onward, cached
   * responses from medical source APIs.
   */
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OtelCollector, OTEL_LEVEL, ENCRYPTION_KEY }) =>
      new RedisTtlCache(
        60 * 60 * 1000,
        OtelCollector,
        { url: REDIS_URL },
        { enabled: true, level: OTEL_LEVEL || 'info' },
        { encryptor: new FieldEncryptor(ENCRYPTION_KEY) }
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

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  ContentSourceProvider: {
    lifetime: Lifetime.Singleton,
    type: PublicCorpusProvider,
    factory: ({ OtelCollector }) => new PublicCorpusProvider(OtelCollector)
  },
  LlmProvider: {
    lifetime: Lifetime.Singleton,
    type: FakeLlmProvider,
    factory: ({ LLM_PROVIDER, EMBEDDING_DIMENSIONS }) => {
      // Only the deterministic development provider exists so far. Failing
      // at startup is deliberate: silently falling back would let a
      // deployment believe it is using a real model when it is not.
      if (LLM_PROVIDER !== 'fake') {
        throw new Error(
          `LLM_PROVIDER '${LLM_PROVIDER}' is not available yet; set LLM_PROVIDER=fake`
        );
      }
      return new FakeLlmProvider(EMBEDDING_DIMENSIONS);
    }
  },
  ComplianceDataService: {
    lifetime: Lifetime.Singleton,
    type: ComplianceDataService,
    // The corpus holds published literature only, and no user-linked entities
    // exist yet (saved searches and history come later), so there is nothing
    // to erase or export against.
    factory: ({ Orm, OtelCollector }) =>
      new ComplianceDataService(Orm, OtelCollector, {})
  },
  RetentionService: {
    lifetime: Lifetime.Singleton,
    type: RetentionService,
    factory: ({ Orm, OtelCollector }) =>
      new RetentionService(Orm, OtelCollector)
  },
  /**
   * One fetcher per source that is fetched over the network. MeSH is loaded
   * from NLM's descriptor file (scripts/load-mesh.ts) instead, because it is
   * published as a single annual file rather than a search API.
   */
  SourceFetchers: {
    lifetime: Lifetime.Singleton,
    type: SourceFetcherRegistry,
    factory: ({ NCBI_TOOL, NCBI_EMAIL, NCBI_API_KEY, OPENFDA_API_KEY }) => {
      const fetchImpl: FetchLike = (url, init) => fetch(url, init);
      const ncbi = {
        tool: NCBI_TOOL,
        email: NCBI_EMAIL,
        apiKey: NCBI_API_KEY || undefined
      };
      return new SourceFetcherRegistry([
        new OpenFdaFetcher(fetchImpl, { apiKey: OPENFDA_API_KEY || undefined }),
        new DailyMedFetcher(fetchImpl),
        new ClinicalTrialsFetcher(fetchImpl),
        new PubMedFetcher(fetchImpl, ncbi),
        new PmcOaFetcher(fetchImpl, ncbi)
      ]);
    }
  },
  IngestionService: {
    lifetime: Lifetime.Scoped,
    type: IngestionService,
    factory: ({ EntityManager, SourceFetchers, LlmProvider, OtelCollector }) =>
      new IngestionService(
        EntityManager,
        SourceFetchers,
        LlmProvider,
        OtelCollector
      )
  },
  LiveRetrievalService: {
    lifetime: Lifetime.Singleton,
    type: LiveRetrievalService,
    factory: ({
      SourceFetchers,
      ContentSourceProvider,
      TtlCache,
      LIVE_RETRIEVAL_TIMEOUT_MS
    }) =>
      new LiveRetrievalService(
        SourceFetchers,
        ContentSourceProvider.describe()
          .filter((source) => source.liveQuery)
          .map((source) => source.id),
        TtlCache,
        { timeoutMs: LIVE_RETRIEVAL_TIMEOUT_MS }
      )
  },
  Reranker: {
    lifetime: Lifetime.Singleton,
    type: LexicalReranker,
    factory: () => new LexicalReranker()
  },
  SearchService: {
    lifetime: Lifetime.Scoped,
    type: SearchService,
    factory: ({
      EntityManager,
      LlmProvider,
      Reranker,
      LiveRetrievalService,
      OtelCollector
    }) =>
      new SearchService(
        EntityManager,
        LlmProvider,
        Reranker,
        LiveRetrievalService,
        OtelCollector
      )
  },
  TopicService: {
    lifetime: Lifetime.Scoped,
    type: TopicService,
    factory: ({ EntityManager, SearchService, OtelCollector }) =>
      new TopicService(EntityManager, SearchService, OtelCollector)
  },
  RedisWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: RedisWorkerOptionsSchema,
    value: {
      pageSize: 10,
      retries: 3,
      interval: 5000
    }
  },
  IngestionJobProducer: {
    lifetime: Lifetime.Scoped,
    type: RedisWorkerProducer<IngestionJob, RedisWorkerOptions>,
    factory: ({ TtlCache, MLSE_INGESTION_QUEUE, RedisWorkerOptions }) =>
      new RedisWorkerProducer(
        MLSE_INGESTION_QUEUE,
        TtlCache,
        RedisWorkerOptions
      )
  },
  IngestionJobConsumer: {
    lifetime: Lifetime.Scoped,
    // Same explicit factory-type assertion ecommerce-stripe uses for its Redis
    // consumer: the function_([...]) schema form does not type-check here.
    type: null as unknown as (
      processEventsFunction: WorkerProcessFunction<IngestionJob>,
      failureHandler: WorkerFailureHandler<IngestionJob>
    ) => RedisWorkerConsumer<IngestionJob, RedisWorkerOptions>,
    factory:
      ({ TtlCache, MLSE_INGESTION_QUEUE, RedisWorkerOptions }) =>
      (
        processEventsFunction: WorkerProcessFunction<IngestionJob>,
        failureHandler: WorkerFailureHandler<IngestionJob>
      ) =>
        new RedisWorkerConsumer(
          MLSE_INGESTION_QUEUE,
          TtlCache,
          RedisWorkerOptions,
          processEventsFunction,
          failureHandler
        )
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
