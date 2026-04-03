import { getEnvVar, safeStringify } from '@forklaunch/common';
import {
  ATTR_HTTP_RESPONSE_STATUS_CODE,
  DocsConfiguration,
  ExpressLikeApplicationOptions,
  ForklaunchExpressLikeApplication,
  ForklaunchRouter,
  generateMcpServer,
  generateOpenApiSpecs,
  isForklaunchRequest,
  isPortBound,
  meta,
  MetricsDefinition,
  OPENAPI_DEFAULT_VERSION,
  OpenTelemetryCollector,
  SessionObject
} from '@forklaunch/core/http';
import { findApplicationRoot } from '@forklaunch/core/environment';
import { AnySchemaValidator } from '@forklaunch/validator';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { apiReference } from '@scalar/express-api-reference';
import crypto from 'crypto';
import express, {
  ErrorRequestHandler,
  Express,
  NextFunction,
  Request,
  RequestHandler,
  Response
} from 'express';
import fs from 'fs';
import { Server } from 'http';
import path from 'path';
import swaggerUi from 'swagger-ui-express';
import { startBunCluster } from './cluster/bun.cluster';
import { startNodeCluster } from './cluster/node.cluster';
import { contentParse } from './middleware/content.parse.middleware';
import { enrichResponseTransmission } from './middleware/enrichResponseTransmission.middleware';
import { ExpressApplicationOptions } from './types/expressOptions.types';

/**
 * Application class that sets up an Express server with Forklaunch routers and middleware.
 * This class extends ForklaunchExpressLikeApplication to provide Express-specific functionality.
 *
 * @template SV - A type that extends AnySchemaValidator for schema validation.
 * @example
 * ```typescript
 * const app = new Application(schemaValidator, openTelemetryCollector);
 * app.listen(3000, () => console.log('Server running on port 3000'));
 * ```
 */
export class Application<
  SV extends AnySchemaValidator,
  SessionSchema extends SessionObject<SV>
> extends ForklaunchExpressLikeApplication<
  SV,
  Express,
  RequestHandler,
  Request,
  Response,
  NextFunction,
  SessionSchema
> {
  private docsConfiguration: DocsConfiguration | undefined;
  private mcpConfiguration:
    | ExpressLikeApplicationOptions<SV, SessionSchema>['mcp']
    | undefined;
  private openapiConfiguration:
    | ExpressLikeApplicationOptions<SV, SessionSchema>['openapi']
    | undefined;
  private hostingConfiguration:
    | ExpressLikeApplicationOptions<SV, SessionSchema>['hosting']
    | undefined;
  /**
   * Creates an instance of Application.
   *
   * @param {SV} schemaValidator - The schema validator for request/response validation.
   * @param {OpenTelemetryCollector<MetricsDefinition>} openTelemetryCollector - Collector for OpenTelemetry metrics.
   * @param {DocsConfiguration} [docsConfiguration] - Optional configuration for API documentation (Swagger/Scalar).
   */
  constructor(
    schemaValidator: SV,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    options?: ExpressApplicationOptions<SV, SessionSchema>
  ) {
    super(
      schemaValidator,
      express(),
      [
        contentParse<SV>(options),
        enrichResponseTransmission as unknown as RequestHandler
      ],
      openTelemetryCollector,
      options
    );

    this.hostingConfiguration = options?.hosting;
    this.docsConfiguration = options?.docs;
    this.mcpConfiguration = options?.mcp;
    this.openapiConfiguration = options?.openapi;
  }

  /**
   * Starts the Express server and sets up API documentation (Swagger/Scalar).
   * This method is overloaded to support various ways of starting the server.
   *
   * @param {number} [port] - The port number to listen on. Defaults to process.env.PORT.
   * @param {string} [hostname] - The hostname to bind to.
   * @param {number} [backlog] - The maximum length of the queue of pending connections.
   * @param {() => void} [callback] - Optional callback to execute when the server starts listening.
   * @returns {Server} - The HTTP server instance.
   *
   * @example
   * ```typescript
   * // Start server on port 3000
   * app.listen(3000);
   *
   * // Start server with callback
   * app.listen(3000, () => console.log('Server started'));
   *
   * // Start server with hostname and port
   * app.listen(3000, 'localhost');
   * ```
   */
  listen(
    port: number,
    hostname: string,
    backlog: number,
    callback?: () => void
  ): Server;
  listen(port: number, hostname: string, callback?: () => void): Server;
  listen(port: number, callback?: () => void): Server;
  listen(callback?: () => void): Server;
  listen(path: string, callback?: () => void): Server;
  listen(handle: unknown, listeningListener?: () => void): Server;
  listen(...args: unknown[]): Server {
    if (process.env.FORKLAUNCH_MODE === 'openapi') {
      const openApiSpec = generateOpenApiSpecs<SV>(
        this.schemaValidator,
        [],
        [],
        this as ForklaunchRouter<SV>,
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

    const port =
      typeof args[0] === 'number' ? args[0] : Number(process.env.PORT);
    const host =
      typeof args[1] === 'string' ? args[1] : (process.env.HOST ?? 'localhost');
    const protocol = (process.env.PROTOCOL as 'http' | 'https') ?? 'http';

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
        contentTypeMapping,
        authenticate
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
        contentTypeMapping,
        authenticate
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
        this as ForklaunchRouter<SV>,
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
          })
        );
      } else if (this.docsConfiguration.type === 'swagger') {
        this.internal.use(
          `/api${process.env.VERSION ? `/${process.env.VERSION}` : ''}${
            process.env.DOCS_PATH ?? '/docs'
          }`,
          swaggerUi.serveFiles(openApi),
          swaggerUi.setup(openApi)
        );
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
        `/api${process.env.VERSION ? `/${process.env.VERSION}` : ''}/openapi-hash`,
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

    const errorHandler: ErrorRequestHandler = (err, req, res, _next) => {
      const statusCode = Number(res.statusCode);
      res.locals.errorMessage = err.message;
      console.error(err);
      res.type('text/plain');
      res
        .status(statusCode >= 400 ? statusCode : 500)
        .send(
          `Internal server error:\n\nCorrelation id: ${
            isForklaunchRequest(req)
              ? req.context.correlationId
              : 'No correlation ID'
          }`
        );
      this.openTelemetryCollector.error(
        err.stack ?? err.message,
        meta({
          [ATTR_HTTP_RESPONSE_STATUS_CODE]: statusCode
        })
      );
    };
    this.internal.use(errorHandler);

    this.internal.use((req, res, next) => {
      res.setHeader('X-Powered-By', 'forklaunch');
      next();
    });

    const { workerCount, ssl, routingStrategy } =
      this.hostingConfiguration ?? {};

    if (workerCount != null && workerCount > 1) {
      if (typeof Bun !== 'undefined') {
        this.openTelemetryCollector.warn(
          `Note: bun clustering is experimental, shimmed, and only works on linux. You may have an inconsistent experience, consider creating multiple hosts.`
        );

        Object.assign(this.internal, {
          basePath: this.basePath
        });

        startBunCluster({
          expressApp: this.internal,
          openTelemetryCollector: this.openTelemetryCollector,
          port,
          host,
          workerCount,
          routingStrategy,
          ssl
        });
      } else {
        startNodeCluster({
          expressApp: this.internal,
          openTelemetryCollector: this.openTelemetryCollector,
          port,
          host,
          workerCount,
          routingStrategy,
          ssl
        });
      }
    }

    return this.internal.listen(...(args as (() => void)[]));
  }
}
