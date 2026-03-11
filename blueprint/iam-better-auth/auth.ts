import { mikroOrmAdapter } from '@forklaunch/better-auth-mikro-orm-fork';
import { Metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { MikroORM } from '@mikro-orm/core';
import { betterAuth, BetterAuthOptions } from 'better-auth';
import { jwt, openAPI } from 'better-auth/plugins';
import { getEnvVar } from '@forklaunch/common';
import { Organization } from './persistence/entities/organization.entity';
import { User } from './persistence/entities/user.entity';

type Plugins = [ReturnType<typeof jwt>, ReturnType<typeof openAPI>];
const plugins: Plugins = [
  jwt({
    jwt: {
      definePayload: async ({ user }) => ({
        sub: user.id,
        email: user.email,
        organizationId: user.organizationId,
        roleIds: user.roleIds
      })
    }
  }),
  openAPI({
    disableDefaultReference: true
  })
];

const userAdditionalFields = {
  firstName: {
    type: 'string',
    required: true
  },
  lastName: {
    type: 'string',
    required: true
  },
  phoneNumber: {
    type: 'string',
    required: false
  },
  organizationId: {
    type: 'string',
    required: false,
    returned: true
  },
  roleIds: {
    type: 'string[]',
    required: false,
    returned: true
  },
  organization: {
    type: 'string',
    required: false,
    returned: false
  },
  roles: {
    type: 'string[]',
    required: false,
    returned: false
  }
} as const;

export const betterAuthConfig = ({
  BETTER_AUTH_BASE_PATH,
  CORS_ORIGINS,
  orm,
  openTelemetryCollector
}: {
  BETTER_AUTH_BASE_PATH: string;
  CORS_ORIGINS: string[];
  orm: MikroORM;
  openTelemetryCollector: OpenTelemetryCollector<Metrics>;
}) => {
  // Construct the base URL for OAuth callbacks
  // Use BETTER_AUTH_URL if set (for Docker/production), otherwise construct from env vars
  const baseURL =
    getEnvVar('BETTER_AUTH_URL') ??
    (() => {
      const protocol = getEnvVar('PROTOCOL') ?? 'http';
      const host = getEnvVar('HOST') ?? 'localhost';
      const port = getEnvVar('PORT') ?? '8000';
      // For Docker: HOST is 0.0.0.0, but browsers need localhost
      const publicHost = host === '0.0.0.0' ? 'localhost' : host;
      return `${protocol}://${publicHost}:${port}`;
    })();

  return {
    baseURL,
    basePath: BETTER_AUTH_BASE_PATH,
    trustedOrigins: CORS_ORIGINS,
    socialProviders: {
      google: {
        clientId: getEnvVar('GOOGLE_CLIENT_ID') ?? '',
        clientSecret: getEnvVar('GOOGLE_CLIENT_SECRET') ?? '',
        redirectURI: `${baseURL}${BETTER_AUTH_BASE_PATH}/callback/google`,
        enabled: !!(
          getEnvVar('GOOGLE_CLIENT_ID') && getEnvVar('GOOGLE_CLIENT_SECRET')
        )
      }
    },
    database: mikroOrmAdapter(orm, {
      options: {
        advanced: {
          database: {
            generateId: false
          }
        },
        user: {
          additionalFields: userAdditionalFields
        }
      }
    }),
    emailAndPassword: {
      enabled: true
    },
    plugins,
    user: {
      additionalFields: userAdditionalFields
    },
    databaseHooks: {
      user: {
        create: {
          before: async (user, ctx) => {
            return {
              data: {
                ...user,
                ...(ctx?.body.organization || ctx?.body.organizationId
                  ? {
                      organization: {
                        $in: [
                          ctx?.body.organizationId || ctx?.body.organization
                        ]
                      }
                    }
                  : {}),
                ...(ctx?.body.roleIds || ctx?.body.roles
                  ? {
                      roles: {
                        $in: ctx?.body.roleIds || ctx?.body.roles
                      }
                    }
                  : {})
              }
            };
          },
          after: async (user) => {
            // If user doesn't have an organization, create one for them
            if (!user.organizationId) {
              try {
                const em = orm.em.fork();
                const orgName = `${user.name || user.email.split('@')[0]}'s Organization`;

                // Create organization using ORM entities
                const organization = em.create(Organization, {
                  name: orgName,
                  domain: `${orgName.toLowerCase().replace(/[^a-z0-9]/g, '-')}.forklaunch.app`,
                  subscription: 'free',
                  status: 'active',
                  createdAt: new Date(),
                  updatedAt: new Date()
                });

                await em.persistAndFlush(organization);

                // Update user with organization
                const userEntity = await em.findOne(User, { id: user.id });
                if (userEntity) {
                  userEntity.organization = organization;
                  await em.persistAndFlush(userEntity);
                }

                openTelemetryCollector.info(
                  'Auto-created organization for new user',
                  {
                    userId: user.id,
                    organizationId: organization.id,
                    organizationName: orgName
                  }
                );
              } catch (error) {
                openTelemetryCollector.error(
                  'Failed to auto-create organization for user',
                  error
                );
              }
            }
          }
        }
      }
    },
    advanced: {
      database: {
        generateId: false
      }
    },
    logger: openTelemetryCollector
  } satisfies BetterAuthOptions;
};

export type BetterAuthConfig = ReturnType<typeof betterAuthConfig>;
export type BetterAuth = ReturnType<typeof betterAuth<BetterAuthConfig>>;
