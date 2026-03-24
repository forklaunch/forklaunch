import { mikroOrmAdapter } from '@forklaunch/better-auth-mikro-orm-fork';
import { PERMISSIONS, ROLES } from '@forklaunch/blueprint-core';
import { Metrics } from '@forklaunch/blueprint-monitoring';
import { getEnvVar } from '@forklaunch/common';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { MikroORM } from '@mikro-orm/core';
import { betterAuth, BetterAuthOptions } from 'better-auth';
import { createAccessControl } from 'better-auth/plugins/access';
import { jwt, openAPI, organization } from 'better-auth/plugins';

const statement = {
  platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
} as const;

const ac = createAccessControl(statement);

const ownerRole = ac.newRole({
  platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
});

const adminRole = ac.newRole({
  platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
});

const editorRole = ac.newRole({
  platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
});

const viewerRole = ac.newRole({
  platform: [PERMISSIONS.PLATFORM_READ]
});

const systemRole = ac.newRole({
  platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
});

const plugins = [
  jwt({
    jwt: {
      definePayload: async ({
        user,
        session
      }: {
        user: Record<string, unknown>;
        session: Record<string, unknown>;
      }) => ({
        sub: user.id,
        email: user.email,
        activeOrganizationId: (session.activeOrganizationId as string) ?? null
      })
    }
  }),
  openAPI({
    disableDefaultReference: true
  }),
  organization({
    allowUserToCreateOrganization: true,
    creatorRole: 'owner',
    membershipLimit: 100,
    invitationExpiresIn: 48 * 60 * 60 * 1000,
    teams: { enabled: true },
    dynamicAccessControl: { enabled: true },
    ac,
    roles: {
      owner: ownerRole,
      [ROLES.ADMIN]: adminRole,
      [ROLES.EDITOR]: editorRole,
      [ROLES.VIEWER]: viewerRole,
      [ROLES.SYSTEM]: systemRole
    },
    schema: {
      organization: {
        additionalFields: {
          domain: {
            type: 'string',
            required: false
          },
          subscription: {
            type: 'string',
            required: false,
            defaultValue: 'free'
          },
          status: {
            type: 'string',
            required: false,
            defaultValue: 'active'
          }
        }
      }
    }
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
  const baseURL =
    getEnvVar('BETTER_AUTH_URL') ??
    (() => {
      const protocol = getEnvVar('PROTOCOL') ?? 'http';
      const host = getEnvVar('HOST') ?? 'localhost';
      const port = getEnvVar('PORT') ?? '8000';
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
    session: {
      expiresIn: 60 * 60 * 24, // 24 hours
      updateAge: 60 * 60 // refresh session if older than 1 hour
    },
    plugins,
    user: {
      additionalFields: userAdditionalFields
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
