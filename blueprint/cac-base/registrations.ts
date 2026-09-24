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
  getCurrentTenantId,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import {
  ComplianceDataService,
  createConfigInjector,
  getEnvVar,
  Lifetime,
  RetentionService
} from '@forklaunch/core/services';
import { ScrubbingService } from '@forklaunch/implementation-cac-base/services';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import mikroOrmOptionsConfig from './mikro-orm.config';
import { AnalyticsService } from './services/analytics.service';
import { ClaimService } from './services/claim.service';
import { CodeSetProviderResolver } from './services/codeSetProviderResolver.service';
import { CodeValidationService } from './services/codeValidation.service';
import { DenialWorklistService } from './services/denialWorklist.service';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'cac',
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
  },
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
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
  // Backs AuthCacheService (§12 item 8, closed) — caches IAM permission/role
  // lookups so protected routes don't re-call IAM on every request. 1hr TTL
  // here is a ceiling; createAuthCacheService (server.ts) applies its own
  // shorter 5min TTL on top per-record, matching billing-base's pattern.
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OtelCollector, OTEL_LEVEL, ENCRYPTION_KEY }) =>
      new RedisTtlCache(
        60 * 60 * 1000,
        OtelCollector,
        {
          url: REDIS_URL
        },
        {
          enabled: true,
          level: OTEL_LEVEL || 'info'
        },
        {
          encryptor: new FieldEncryptor(ENCRYPTION_KEY)
        }
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
  // Replaces the old always-mock CodeSetProvider Singleton — the per-org
  // runtime feature gate (§5, plan §12 item 11) resolves Mock vs. real CPT
  // fresh per request, based on that organization's own CodeSetLicense.
  CodeSetProviderResolver: {
    lifetime: Lifetime.Scoped,
    type: CodeSetProviderResolver,
    factory: ({ EntityManager, OtelCollector }) =>
      new CodeSetProviderResolver(EntityManager, OtelCollector)
  },
  CodeValidationService: {
    lifetime: Lifetime.Scoped,
    type: CodeValidationService,
    factory: ({ EntityManager, OtelCollector }) =>
      new CodeValidationService(EntityManager, OtelCollector)
  },
  ScrubbingService: {
    lifetime: Lifetime.Singleton,
    type: ScrubbingService,
    factory: () => new ScrubbingService()
  },
  ClaimService: {
    lifetime: Lifetime.Scoped,
    type: ClaimService,
    factory: ({
      EntityManager,
      ScrubbingService,
      CodeSetProviderResolver,
      CodeValidationService,
      OtelCollector
    }) =>
      new ClaimService(
        EntityManager,
        ScrubbingService,
        CodeSetProviderResolver,
        CodeValidationService,
        OtelCollector
      )
  },
  DenialWorklistService: {
    lifetime: Lifetime.Scoped,
    type: DenialWorklistService,
    factory: ({ EntityManager, OtelCollector }) =>
      new DenialWorklistService(EntityManager, OtelCollector)
  },
  AnalyticsService: {
    lifetime: Lifetime.Scoped,
    type: AnalyticsService,
    factory: ({ EntityManager, OtelCollector }) =>
      new AnalyticsService(EntityManager, OtelCollector)
  },
  ComplianceDataService: {
    lifetime: Lifetime.Singleton,
    type: ComplianceDataService,
    // Erase/export requests are keyed by the Patient record itself — the
    // three entities below are the only ones with a *direct* field back to
    // a patient id. Diagnosis/Charge/Remittance/Denial only reach a patient
    // by walking through Encounter/Claim, which the framework's generic
    // compliance service doesn't cascade through (single-hop by design) —
    // out of scope for this phase, tracked in plan/cac/ §12 if it needs
    // solving later.
    //
    // `ComplianceDataService.erase`/`export` fork their own EntityManager
    // internally (`this.orm.em.fork()`) with no way to pass tenant context
    // in — the published service has no such hook, and adding one would
    // mean cutting a new @forklaunch/core release before this module could
    // even use it (same trap `@forklaunch/interfaces-cac` hit earlier in
    // this PR). So instead of the bare `Orm`, this passes an adapter whose
    // `em.fork()` wraps the real fork with `wrapEmWithTenantContext`,
    // reading whatever tenant is ambient (via `getCurrentTenantId()`) at
    // the moment `erase`/`export` calls it. compliance.controller.ts binds
    // that ambient tenant with `withEncryptionContext(tenantId, ...)`
    // before calling in. Without this, PHI fields (Patient.firstName/
    // lastName/dateOfBirth/ssn, Insurance.memberId) decrypt under the
    // bare/no-tenant key instead of the organization's real one — see
    // claim.controller.ts's tenant-context comment and
    // framework/core/src/persistence/tenantEm.ts's doc comment for why
    // this has to be the wrapped-EM mechanism, not just an outer
    // `withEncryptionContext` around the whole call: pg's connection pool
    // callbacks don't reliably carry a single ambient AsyncLocalStorage
    // scope across the several separate queries erase/export run.
    factory: ({ Orm, OtelCollector }) =>
      new ComplianceDataService(
        {
          em: {
            fork: (options?: ForkOptions) =>
              wrapEmWithTenantContext(
                Orm.em.fork(options),
                getCurrentTenantId() || undefined
              )
          },
          getMetadata: () => Orm.getMetadata()
        },
        OtelCollector,
        {
          Patient: 'id',
          Insurance: 'patient',
          Encounter: 'patient',
          Claim: 'patient'
        }
      )
  },
  RetentionService: {
    lifetime: Lifetime.Singleton,
    type: RetentionService,
    factory: ({ Orm, OtelCollector }) =>
      new RetentionService(Orm, OtelCollector)
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
