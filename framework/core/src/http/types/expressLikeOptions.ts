import { AnySchemaValidator } from '@forklaunch/validator';
import { CorsOptions } from 'cors';
import { FastMCP } from 'fastmcp';
import http from 'http';
import { JWTPayload } from 'jose';
import { ForklaunchRequest, MapSessionSchema } from './apiDefinition.types';
import { SessionObject } from './contractDetails.types';
import { DocsConfiguration } from './docsConfiguration.types';

/**
 * Default subscription data structure.
 * Applications can override this with their own subscription type.
 */
export type DefaultSubscriptionData = {
  subscriptionId: string;
  planId: string;
  planName: string;
  status: string;
  currentPeriodEnd: Date;
} | null;

/**
 * Options for global authentication in Express-like applications.
 *
 * @template SV - The schema validator type.
 * @template SessionSchema - The session schema type.
 * @template SubscriptionData - The subscription data type (defaults to DefaultSubscriptionData, must extend Record<string, unknown> | null).
 *
 * Can be `false` to disable authentication, or an object specifying session schema and
 * functions to surface scopes, permissions, roles, subscription, and features from the JWT/session.
 */
export type ExpressLikeGlobalAuthOptions<
  SV extends AnySchemaValidator,
  SessionSchema extends Record<string, unknown>,
  SubscriptionData extends Record<string, unknown> | null =
    DefaultSubscriptionData
> =
  | false
  | {
      /**
       * Optional session schema for the authentication context.
       */
      sessionSchema?: SessionSchema;
      /**
       * Optional array describing the scope hierarchy for authorization.
       */
      scopeHeirarchy?: string[];
      /**
       * Function to extract a set of scopes from the JWT payload and request.
       */
      surfaceScopes?: (
        payload: JWTPayload & SessionSchema,
        req?: ForklaunchRequest<
          SV,
          Record<string, string>,
          Record<string, unknown>,
          Record<string, unknown>,
          Record<string, unknown>,
          string,
          SessionSchema
        >
      ) => Set<string> | Promise<Set<string>>;
      /**
       * Function to extract a set of permissions from the JWT payload and request.
       */
      surfacePermissions?: (
        payload: JWTPayload & SessionSchema,
        req?: ForklaunchRequest<
          SV,
          Record<string, string>,
          Record<string, unknown>,
          Record<string, unknown>,
          Record<string, unknown>,
          string,
          SessionSchema
        >
      ) => Set<string> | Promise<Set<string>>;
      /**
       * Function to extract a set of roles from the JWT payload and request.
       */
      surfaceRoles?: (
        payload: JWTPayload & SessionSchema,
        req?: ForklaunchRequest<
          SV,
          Record<string, string>,
          Record<string, unknown>,
          Record<string, unknown>,
          Record<string, unknown>,
          string,
          SessionSchema
        >
      ) => Set<string> | Promise<Set<string>>;
      /**
       * Function to extract subscription data from the JWT payload and request.
       * Returns subscription information (status, plan, etc.) or null if no subscription.
       */
      surfaceSubscription?: (
        payload: JWTPayload & SessionSchema,
        req?: ForklaunchRequest<
          SV,
          Record<string, string>,
          Record<string, unknown>,
          Record<string, unknown>,
          Record<string, unknown>,
          string,
          SessionSchema
        >
      ) => SubscriptionData | Promise<SubscriptionData>;
      /**
       * Function to extract a set of feature flags from the JWT payload and request.
       * Returns features available to the organization based on their billing plan.
       */
      surfaceFeatures?: (
        payload: JWTPayload & SessionSchema,
        req?: ForklaunchRequest<
          SV,
          Record<string, string>,
          Record<string, unknown>,
          Record<string, unknown>,
          Record<string, unknown>,
          string,
          SessionSchema
        >
      ) => Set<string> | Promise<Set<string>>;
    };

/**
 * Schema-aware version of ExpressLikeGlobalAuthOptions.
 *
 * @template SV - The schema validator type.
 * @template SessionSchema - The session object type.
 * @template SubscriptionData - The subscription data type.
 */
export type ExpressLikeSchemaGlobalAuthOptions<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>,
  SubscriptionData extends Record<string, unknown> | null =
    DefaultSubscriptionData
> = ExpressLikeGlobalAuthOptions<
  SV,
  MapSessionSchema<SV, SessionSchema>,
  SubscriptionData
>;

/**
 * Options for configuring an Express-like router.
 *
 * @template SV - The schema validator type.
 * @template SessionSchema - The session object type.
 * @template SubscriptionData - The subscription data type.
 */
export type ExpressLikeRouterOptions<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>,
  SubscriptionData extends Record<string, unknown> | null =
    DefaultSubscriptionData
> = {
  /**
   * Authentication options for the router.
   */
  auth?: ExpressLikeSchemaGlobalAuthOptions<
    SV,
    SessionSchema,
    SubscriptionData
  >;
  /**
   * Validation options for request and response.
   * Can be `false` to disable validation, or an object to configure request/response validation levels.
   */
  validation?:
    | false
    | {
        /**
         * Request validation mode: 'none', 'warning', or 'error'.
         */
        request?: 'none' | 'warning' | 'error';
        /**
         * Response validation mode: 'none', 'warning', or 'error'.
         */
        response?: 'none' | 'warning' | 'error';
      };
  /**
   * OpenAPI documentation options.
   */
  openapi?: boolean;
  /**
   * MCP options.
   */
  mcp?: boolean;
};

/**
 * Options for configuring an Express-like application.
 *
 * @template SV - The schema validator type.
 * @template SessionSchema - The session object type.
 * @template SubscriptionData - The subscription data type.
 */
export type ExpressLikeApplicationOptions<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>,
  SubscriptionData extends Record<string, unknown> | null =
    DefaultSubscriptionData
> = Omit<
  ExpressLikeRouterOptions<SV, SessionSchema, SubscriptionData>,
  'openapi' | 'mcp'
> & {
  /**
   * Documentation configuration.
   */
  docs?: DocsConfiguration;
  /**
   * Hosting/server options.
   */
  hosting?: {
    /**
     * SSL configuration or boolean to enable/disable SSL.
     */
    ssl?: {
      /**
       * SSL certificate as a string.
       */
      certFile: string;
      /**
       * SSL key as a string.
       */
      keyFile: string;
      /**
       * SSL CA as a string.
       */
      caFile: string;
    };
    /**
     * Number of worker processes to spawn.
     */
    workerCount?: number;
    /**
     * Routing strategy to use.
     */
    routingStrategy?: 'round-robin' | 'sticky' | 'random';
  };
  /**
   * FastMCP (management/control plane) options.
   * Can be `false` to disable, or an object to configure.
   */
  mcp?:
    | false
    | {
        /**
         * Port for the MCP server.
         */
        port?: number;
        /**
         * Endpoint for the MCP server.
         */
        path?: `/${string}`;
        /**
         * Session handling when MCP is served on the application port.
         * `stateless` (default) lets any worker or task answer any request.
         * `stateful` keeps sessions in process memory, so it needs a single
         * worker and, behind a load balancer, sticky sessions.
         */
        sessions?: 'stateless' | 'stateful';
        /**
         * Stateful sessions with no request for this long are closed.
         * Defaults to 15 minutes.
         */
        sessionIdleTimeoutMs?: number;
        /**
         * Additional options for FastMCP.
         */
        options?: ConstructorParameters<typeof FastMCP>[0];
        /**
         * Optional authentication callback for validating MCP requests.
         * Called by FastMCP for each request to verify the session token.
         * Return user info object if authenticated, undefined otherwise.
         * Example: { email: 'user@example.com', name: 'User' }
         */
        authenticate?: (
          request: http.IncomingMessage
        ) => Promise<Record<string, unknown> | undefined>;
        /**
         * Additional tools to register with the MCP server.
         */
        additionalTools?: (mcpServer: FastMCP) => void;
        /**
         * Content type mapping for the MCP server.
         */
        contentTypeMapping?: Record<string, string>;
        /**
         * Version of the MCP server.
         */
        version?: `${number}.${number}.${number}`;
      };
  /**
   * OpenAPI documentation options.
   * Can be `false` to disable, or an object to configure.
   */
  openapi?:
    | false
    | {
        /**
         * Path to serve the OpenAPI docs (e.g., '/openapi').
         */
        path?: `/${string}`;
        /**
         * Title for the OpenAPI documentation.
         */
        title?: string;
        /**
         * Description for the OpenAPI documentation.
         */
        description?: string;
        /**
         * Contact information for the API.
         */
        contact?: {
          /**
           * Contact name.
           */
          name?: string;
          /**
           * Contact email.
           */
          email?: string;
        };
        /**
         * Whether to enable discrete versioning for OpenAPI docs.
         */
        discreteVersions?: boolean;
      };
  /**
   * CORS configuration options, used exactly as given.
   *
   * Leave this out to get the secure default: only the origins listed in the
   * `CORS_ORIGINS` environment variable (comma separated) are allowed; when
   * that is unset, no cross-origin request is allowed in production and only
   * localhost origins are allowed elsewhere.
   *
   * WARNING: an object without `origin`, including an empty `cors: {}`,
   * allows EVERY origin (`Access-Control-Allow-Origin: *`). Name the origins:
   * `cors: { origin: ['https://app.example.com'] }`.
   */
  cors?: CorsOptions;
};
