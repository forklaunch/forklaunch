import {
  array,
  ExpressApplicationOptions,
  number,
  optional,
  promise,
  schemaValidator,
  SchemaValidator,
  string,
  type
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector, SessionObject } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import {
  BaseOrganizationService,
  BasePermissionService,
  BaseRoleService,
  BaseUserService
} from '@forklaunch/implementation-iam-base/services';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import { betterAuth } from 'better-auth';
import { BetterAuth, betterAuthConfig } from './auth';
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
  BETTER_AUTH_BASE_PATH: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('BETTER_AUTH_BASE_PATH') ?? '/api/auth'
  },
  CORS_ORIGINS: {
    lifetime: Lifetime.Singleton,
    type: array(string),
    value: getEnvVar('CORS_ORIGINS')?.split(',')
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
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
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
    factory: ({ EntityManager, OpenTelemetryCollector }, resolve, context) =>
      new BaseOrganizationService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OpenTelemetryCollector,
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
    factory: ({ EntityManager, OpenTelemetryCollector }, resolve, context) =>
      new BasePermissionService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        () => resolve('RoleService', context),
        OpenTelemetryCollector,
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
    factory: ({ EntityManager, OpenTelemetryCollector }, resolve, context) =>
      new BaseRoleService(
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OpenTelemetryCollector,
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
    factory: ({ EntityManager, OpenTelemetryCollector }, resolve, context) =>
      new BaseUserService(
        EntityManager,
        () => resolve('RoleService', context),
        () => resolve('OrganizationService', context),
        OpenTelemetryCollector,
        schemaValidator,
        {
          UserMapper,
          CreateUserMapper,
          UpdateUserMapper
        }
      )
  }
});

//! defines the express application options for the application
const expressApplicationOptions = serviceDependencies.chain({
  BetterAuth: {
    lifetime: Lifetime.Singleton,
    type: type<unknown>(),
    factory: ({
      BETTER_AUTH_BASE_PATH,
      CORS_ORIGINS,
      MikroORM,
      OpenTelemetryCollector
    }) =>
      betterAuth(
        betterAuthConfig({
          BETTER_AUTH_BASE_PATH,
          CORS_ORIGINS,
          orm: MikroORM,
          openTelemetryCollector: OpenTelemetryCollector
        })
      ) as BetterAuth
  },
  ExpressApplicationOptions: {
    lifetime: Lifetime.Singleton,
    type: promise(
      type<
        ExpressApplicationOptions<
          SchemaValidator,
          SessionObject<SchemaValidator>
        >
      >()
    ),
    factory: async ({
      BETTER_AUTH_BASE_PATH,
      CORS_ORIGINS,
      BetterAuth,
      UserService
    }) => {
      const betterAuthOpenAPIContent = await (
        BetterAuth as BetterAuth
      ).api.generateOpenAPISchema();

      const options: ExpressApplicationOptions<
        SchemaValidator,
        SessionObject<SchemaValidator>
      > = {
        auth: {
          surfacePermissions: async (payload) => {
            if (!payload.sub) {
              return new Set();
            }
            return new Set(
              (
                await UserService.surfacePermissions({
                  id: payload.sub
                })
              ).map((permission) => permission.slug)
            );
          },
          surfaceRoles: async (payload) => {
            if (!payload.sub) {
              return new Set();
            }
            return new Set(
              (
                await UserService.surfaceRoles({
                  id: payload.sub
                })
              ).map((role) => role.name)
            );
          }
        },
        cors: {
          origin: CORS_ORIGINS,
          methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
          credentials: true
        },
        docs: {
          type: 'scalar' as const,
          sources: [
            {
              title: 'BetterAuth',
              content: {
                ...betterAuthOpenAPIContent,
                paths: Object.fromEntries(
                  Object.entries(betterAuthOpenAPIContent.paths).map(
                    ([key, value]) => [`${BETTER_AUTH_BASE_PATH}${key}`, value]
                  )
                )
              }
            }
          ]
        }
      };

      return options;
    }
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer: (envFilePath: string) => {
  ci: ReturnType<typeof expressApplicationOptions.validateConfigSingletons>;
  tokens: ReturnType<typeof expressApplicationOptions.tokens>;
} = (envFilePath: string) => ({
  ci: expressApplicationOptions.validateConfigSingletons(envFilePath),
  tokens: expressApplicationOptions.tokens()
});
