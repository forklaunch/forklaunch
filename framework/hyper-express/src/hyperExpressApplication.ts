import { getEnvVar, safeStringify } from '@forklaunch/common';
import {
  ATTR_HTTP_RESPONSE_STATUS_CODE,
  DocsConfiguration,
  ForklaunchExpressLikeApplication,
  ForklaunchRouter,
  generateMcpServer,
  generateOpenApiSpecs,
  isForklaunchRequest,
  isPortBound,
  MetricsDefinition,
  OPENAPI_DEFAULT_VERSION,
  OpenTelemetryCollector,
  SessionObject
} from '@forklaunch/core/http';
import { findApplicationRoot } from '@forklaunch/core/environment';
import {
  MiddlewareHandler,
  MiddlewareNext,
  Request,
  Response,
  Server
} from '@forklaunch/hyper-express-fork';
import { AnySchemaValidator } from '@forklaunch/validator';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { apiReference } from '@scalar/express-api-reference';
import crypto from 'crypto';
import fs from 'fs';
import path from 'path';
import * as uWebsockets from 'uWebSockets.js';
import { startHyperExpressCluster } from './cluster/hyperExpress.cluster';
import { contentParse } from './middleware/contentParse.middleware';
import { enrichResponseTransmission } from './middleware/enrichResponseTransmission.middleware';
import { swagger, swaggerRedirect } from './middleware/swagger.middleware';
import { ExpressApplicationOptions } from './types/hyperExpressOptions.types';

/**
 * Represents an application built on top of Hyper-Express and Forklaunch.
 * This class provides a high-performance web server implementation using uWebSockets.js
 * with Forklaunch's schema validation and API documentation capabilities.
 *
 * @template SV - Schema validator type extending AnySchemaValidator
 *
 * @example
 * ```typescript
 * const app = new Application(schemaValidator, openTelemetryCollector);
 *
 * app.get('/users', {
 *   responses: {
 *     200: { type: 'array', items: { type: 'object' } }
 *   }
 * }, async (req, res) => {
 *   res.json(await getUsers());
 * });
 *
 * await app.listen(3000);
 * ```
 */
export class Application<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>
> extends ForklaunchExpressLikeApplication<
  SV,
  Server,
  MiddlewareHandler,
  Request<Record<string, unknown>>,
  Response<Record<string, unknown>>,
  MiddlewareNext,
  SessionSchema
> {
  private docsConfiguration: DocsConfiguration | undefined;
  private mcpConfiguration:
    | ExpressApplicationOptions<SV, SessionSchema>['mcp']
    | undefined;
  private openapiConfiguration:
    | ExpressApplicationOptions<SV, SessionSchema>['openapi']
    | undefined;
  private hostingConfiguration:
    | ExpressApplicationOptions<SV, SessionSchema>['hosting']
    | undefined;
  /**
   * Creates an instance of the Application class.
   *
   * @param {SV} schemaValidator - Schema validator for request/response validation
   * @param {OpenTelemetryCollector<MetricsDefinition>} openTelemetryCollector - Collector for OpenTelemetry metrics
   * @param {DocsConfiguration} [docsConfiguration] - Optional configuration for API documentation (Swagger/Scalar)
   *
   * @example
   * ```typescript
   * const app = new Application(
   *   new SchemaValidator(),
   *   new OpenTelemetryCollector(),
   *   { type: 'swagger' }
   * );
   * ```
   */
  constructor(
    schemaValidator: SV,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    configurationOptions?: ExpressApplicationOptions<SV, SessionSchema>
  ) {
    super(
      schemaValidator,
      new Server({
        key_file_name: configurationOptions?.hosting?.ssl?.keyFile,
        cert_file_name: configurationOptions?.hosting?.ssl?.certFile,
        ...configurationOptions?.server
      }),
      [
        contentParse<SV>(configurationOptions),
        enrichResponseTransmission as unknown as MiddlewareHandler
      ],
      openTelemetryCollector,
      configurationOptions
    );

    this.hostingConfiguration = configurationOptions?.hosting;
    this.docsConfiguration = configurationOptions?.docs;
    this.mcpConfiguration = configurationOptions?.mcp;
    this.openapiConfiguration = configurationOptions?.openapi;
  }

  /**
   * Starts the server and sets up API documentation.
   * Supports multiple listening configurations including port, host, and UNIX socket path.
   * Automatically sets up Swagger or Scalar documentation based on configuration.
   *
   * @param {number | string} arg0 - Port number or UNIX socket path
   * @param {string | Function} [arg1] - Host name or callback function
   * @param {Function} [arg2] - Callback function when host is specified
   * @returns {Promise<uWebsockets.us_listen_socket>} Promise resolving to the listening socket
   *
   * @example
   * ```typescript
   * // Listen on port 3000
   * await app.listen(3000);
   *
   * // Listen on specific host and port
   * await app.listen(3000, 'localhost');
   *
   * // Listen on UNIX socket
   * await app.listen('/tmp/app.sock');
   *
   * // With callback
   * await app.listen(3000, (socket) => {
   *   console.log('Server started');
   * });
   * ```
   */
  async listen(
    port: number,
    callback?: (listen_socket: uWebsockets.us_listen_socket) => void
  ): Promise<uWebsockets.us_listen_socket>;
  async listen(
    port: number,
    host?: string,
    callback?: (listen_socket: uWebsockets.us_listen_socket) => void
  ): Promise<uWebsockets.us_listen_socket>;
  async listen(
    unix_path: string,
    callback?: (listen_socket: uWebsockets.us_listen_socket) => void
  ): Promise<uWebsockets.us_listen_socket>;
  async listen(
    arg0: number | string,
    arg1?: string | ((listen_socket: uWebsockets.us_listen_socket) => void),
    arg2?: (listen_socket: uWebsockets.us_listen_socket) => void
  ): Promise<uWebsockets.us_listen_socket> {
    this.validateAllRoutes();

    if (process.env.FORKLAUNCH_MODE === 'openapi') {
      const openApiSpec = generateOpenApiSpecs<SV>(
        this.schemaValidator,
        [],
        [],
        this,
        this.openapiConfiguration
      );
      const serviceName = process.env.OTEL_SERVICE_NAME || 'unknown-service';
      const appRoot = findApplicationRoot(process.cwd());
      const outputPath =
        process.env.FORKLAUNCH_OPENAPI_OUTPUT ||
        path.join(
          appRoot,
          '.forklaunch',
          'openapi',
          serviceName,
          'openapi.json'
        );
      fs.mkdirSync(path.dirname(outputPath), { recursive: true });
      fs.writeFileSync(
        outputPath,
        JSON.stringify(
          {
            ...openApiSpec,
            '': openApiSpec[OPENAPI_DEFAULT_VERSION]
          },
          null,
          2
        )
      );
      process.exit(0);
    }

    if (typeof arg0 === 'number') {
      const port = arg0 || Number(process.env.PORT);
      const protocol = (process.env.PROTOCOL || 'http') as 'http' | 'https';
      const host =
        typeof arg1 === 'string' ? arg1 : process.env.HOST || '0.0.0.0';

      this.internal.set_error_handler((req, res, err) => {
        const statusCode = Number(res.statusCode);
        res.locals.errorMessage = err.message;
        res.type('text/plain');
        res
          .status(statusCode && statusCode >= 400 ? statusCode : 500)
          .send(
            `Internal server error:\n\nCorrelation id: ${
              isForklaunchRequest(req)
                ? req.context.correlationId
                : 'No correlation ID'
            }`
          );
        this.openTelemetryCollector.error(err.stack ?? err.message, {
          [ATTR_HTTP_RESPONSE_STATUS_CODE]: statusCode ?? 500
        });
      });

      this.internal.any('/*', (req, res, next) => {
        res.setHeader('X-Powered-By', 'forklaunch');
        next();
      });

      if (
        this.schemaValidator instanceof ZodSchemaValidator &&
        this.mcpConfiguration !== false
      ) {
        const {
          port: mcpPort,
          options,
          path: mcpPath,
          version,
          additionalTools,
          contentTypeMapping
        } = this.mcpConfiguration ?? {};
        const zodSchemaValidator = this.schemaValidator;
        const finalMcpPort = mcpPort ?? port + 2000;

        const mcpServer = generateMcpServer(
          zodSchemaValidator,
          protocol,
          host,
          port,
          version ?? '1.0.0',
          this as unknown as ForklaunchRouter<ZodSchemaValidator>,
          this.mcpConfiguration,
          options,
          contentTypeMapping
        );

        if (additionalTools) {
          additionalTools(mcpServer);
        }

        if (
          this.hostingConfiguration?.workerCount &&
          this.hostingConfiguration.workerCount > 1
        ) {
          isPortBound(finalMcpPort, host).then((isBound) => {
            if (!isBound) {
              mcpServer.start({
                httpStream: {
                  host,
                  endpoint: mcpPath ?? '/mcp',
                  port: finalMcpPort
                },
                transportType: 'httpStream'
              });
            }
          });
        } else {
          mcpServer.start({
            httpStream: {
              host,
              endpoint: mcpPath ?? '/mcp',
              port: finalMcpPort
            },
            transportType: 'httpStream'
          });
        }
      }

      if (this.openapiConfiguration !== false) {
        const openApiServerUrls = getEnvVar('DOCS_SERVER_URLS')?.split(',') ?? [
          `${protocol}://${host}:${port}`
        ];
        const openApiServerDescriptions = getEnvVar(
          'DOCS_SERVER_DESCRIPTIONS'
        )?.split(',') ?? ['Main Server'];

        const openApi = generateOpenApiSpecs<SV>(
          this.schemaValidator,
          openApiServerUrls,
          openApiServerDescriptions,
          this,
          this.openapiConfiguration
        );

        if (
          this.docsConfiguration == null ||
          this.docsConfiguration.type === 'scalar'
        ) {
          this.internal.use(
            `/api${process.env.VERSION ? `/${process.env.VERSION}` : ''}${
              process.env.DOCS_PATH ?? '/docs'
            }`,
            apiReference({
              ...this.docsConfiguration,
              sources: [
                {
                  content: openApi[OPENAPI_DEFAULT_VERSION],
                  title: 'API Reference'
                },
                ...Object.entries(openApi).map(([version, spec]) => ({
                  content:
                    typeof this.openapiConfiguration === 'boolean'
                      ? spec
                      : this.openapiConfiguration?.discreteVersions === false
                        ? {
                            ...openApi[OPENAPI_DEFAULT_VERSION],
                            ...spec
                          }
                        : spec,
                  title: `API Reference - ${version}`
                })),
                ...(this.docsConfiguration?.sources ?? [])
              ]
            }) as unknown as MiddlewareHandler
          );
        } else if (this.docsConfiguration?.type === 'swagger') {
          const swaggerPath = `/api${process.env.VERSION ? `/${process.env.VERSION}` : ''}${
            process.env.DOCS_PATH ?? '/docs'
          }`;
          Object.entries(openApi).forEach(([version, spec]) => {
            const versionPath = encodeURIComponent(`${swaggerPath}/${version}`);
            this.internal.use(versionPath, swaggerRedirect(versionPath));
            this.internal.get(`${versionPath}/*`, swagger(versionPath, spec));
          });
        }

        this.internal.get(
          `/api${process.env.VERSION ? `/${process.env.VERSION}` : ''}/openapi`,
          (_, res) => {
            res.type('application/json');
            res.json({
              latest: openApi[OPENAPI_DEFAULT_VERSION],
              ...Object.fromEntries(
                Object.entries(openApi).map(([version, spec]) => [
                  `v${version}`,
                  typeof this.openapiConfiguration === 'boolean'
                    ? spec
                    : this.openapiConfiguration?.discreteVersions === false
                      ? {
                          ...openApi[OPENAPI_DEFAULT_VERSION],
                          ...spec
                        }
                      : spec
                ])
              )
            });
          }
        );

        this.internal.get(
          `/api${process.env.VERSION ? `/${process.env.VERSION}` : ''}/openapi/:id`,
          async (req, res) => {
            res.type('application/json');
            if (req.params.id === 'latest') {
              res.json(openApi[OPENAPI_DEFAULT_VERSION]);
            } else {
              if (openApi[req.params.id] == null) {
                res.status(404).send('Not Found');
                return;
              }

              res.json(
                typeof this.openapiConfiguration === 'boolean'
                  ? openApi[req.params.id]
                  : this.openapiConfiguration?.discreteVersions === false
                    ? {
                        ...openApi[OPENAPI_DEFAULT_VERSION],
                        ...openApi[req.params.id]
                      }
                    : openApi[req.params.id]
              );
            }
          }
        );

        this.internal.get(
          `/api/${process.env.VERSION ?? 'v1'}/openapi-hash`,
          async (_, res) => {
            const hash = await crypto
              .createHash('sha256')
              .update(safeStringify(openApi))
              .digest('hex');
            res.send(hash);
          }
        );
      }

      this.internal.get('/health', (_, res) => {
        res.send('OK');
      });

      const { workerCount, ssl, routingStrategy } =
        this.hostingConfiguration ?? {};

      if (workerCount != null && workerCount > 1) {
        this.openTelemetryCollector.warn(
          'Clustering with hyper-express will default to kernel-level routing.'
        );
        startHyperExpressCluster({
          expressApp: this.internal,
          openTelemetryCollector: this.openTelemetryCollector,
          port,
          host,
          workerCount,
          routingStrategy,
          ssl
        });
      }

      if (arg1 && typeof arg1 === 'string') {
        return this.internal.listen(port, arg1, arg2);
      } else if (arg1 && typeof arg1 === 'function') {
        return this.internal.listen(port, arg1);
      }
    }

    return this.internal.listen(
      arg0 as string,
      arg1 as (listen_socket: uWebsockets.us_listen_socket) => void
    );
  }
}
