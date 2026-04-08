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
  Lifetime,
  RetentionService
} from '@forklaunch/core/services';
import {
  BaseOrganizationService,
  BasePermissionService,
  BaseRoleService,
  BaseUserService
} from '@forklaunch/implementation-iam-base/services';
import { wrapEmWithTenantContext } from '@forklaunch/core/persistence';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import { OrganizationStatus } from './domain/enum/organizationStatus.enum';
import {
  CreateOrganizationMapper,
  OrganizationMapper,
  UpdateOrganizationMapper
} from './domain/mappers/organization.mappers';
import {
  CreatePermissionMapper,
  PermissionMapper,
  UpdatePermissionMapper
} from './domain/mappers/permission.mappers';
import {
  CreateRoleMapper,
  RoleEntityMapper,
  RoleMapper,
  UpdateRoleMapper
} from './domain/mappers/role.mappers';
import {
  CreateUserMapper,
  UpdateUserMapper,
  UserMapper
} from './domain/mappers/user.mappers';
import {
  OrganizationDtoTypes,
  OrganizationMapperTypes,
  PermissionDtoTypes,
  PermissionMapperTypes,
  RoleDtoTypes,
  RoleMapperTypes,
  UserDtoTypes,
  UserMapperTypes
} from './domain/types/iamMappers.types';
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
      name: 'iam',
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
      )
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  OrganizationService: {
    lifetime: Lifetime.Scoped,
    type: BaseOrganizationService<
      SchemaValidator,
      typeof OrganizationStatus,
      OrganizationMapperTypes,
      OrganizationDtoTypes
    >,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseOrganizationService(
        context?.entityManagerOptions
          ? resolve?.('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        {
          OrganizationMapper,
          CreateOrganizationMapper,
          UpdateOrganizationMapper
        }
      )
  },
  PermissionService: {
    lifetime: Lifetime.Scoped,
    type: BasePermissionService<
      SchemaValidator,
      PermissionMapperTypes,
      PermissionDtoTypes
    >,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BasePermissionService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        () => resolve('RoleService', context),
        OtelCollector,
        schemaValidator,
        {
          PermissionMapper,
          CreatePermissionMapper,
          UpdatePermissionMapper,
          RoleEntityMapper
        }
      )
  },
  RoleService: {
    lifetime: Lifetime.Scoped,
    type: BaseRoleService<SchemaValidator, RoleMapperTypes, RoleDtoTypes>,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseRoleService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        {
          RoleMapper,
          CreateRoleMapper,
          UpdateRoleMapper
        }
      )
  },
  UserService: {
    lifetime: Lifetime.Scoped,
    type: BaseUserService<
      SchemaValidator,
      typeof OrganizationStatus,
      UserMapperTypes,
      UserDtoTypes
    >,
    factory: ({ EntityManager, OtelCollector }, context, resolve) =>
      new BaseUserService(
        EntityManager,
        () => resolve('RoleService', context),
        () => resolve('OrganizationService', context),
        OtelCollector,
        schemaValidator,
        {
          UserMapper,
          CreateUserMapper,
          UpdateUserMapper
        }
      )
  },
  ComplianceDataService: {
    lifetime: Lifetime.Singleton,
    type: ComplianceDataService,
    factory: ({ Orm, OtelCollector }) =>
      new ComplianceDataService(Orm, OtelCollector, {
        User: 'id'
      })
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
