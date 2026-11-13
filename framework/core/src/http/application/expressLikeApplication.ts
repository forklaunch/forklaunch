import { EmptyObject } from '@forklaunch/common';
import { AnySchemaValidator } from '@forklaunch/validator';
import { ExpressLikeRouter } from '../interfaces/expressLikeRouter.interface';
import {
  cors,
  resolveCorsOptions
} from '../middleware/request/cors.middleware';
import { ForklaunchExpressLikeRouter } from '../router/expressLikeRouter';
import { OpenTelemetryCollector } from '../telemetry/openTelemetryCollector';
import { SessionObject } from '../types/contractDetails.types';
import { ExpressLikeApplicationOptions } from '../types/expressLikeOptions';
import { MetricsDefinition } from '../types/openTelemetryCollector.types';

// One startup warning per process, however many applications are built.
let warnedAboutCorsDefaults = false;

/**
 * ForklaunchExpressLikeApplication class that sets up routes and middleware for an Express-like application, for use with controller/routes pattern.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @template Server - The server type.
 */
export abstract class ForklaunchExpressLikeApplication<
  SV extends AnySchemaValidator,
  Server extends ExpressLikeRouter<RouterHandler, Server>,
  RouterHandler,
  BaseRequest,
  BaseResponse,
  NextFunction,
  Session extends SessionObject<SV>,
  FetchMap extends Record<string, unknown> = EmptyObject,
  Sdk extends Record<string, unknown> = EmptyObject
> extends ForklaunchExpressLikeRouter<
  SV,
  '/',
  RouterHandler,
  Server,
  BaseRequest,
  BaseResponse,
  NextFunction,
  Session,
  FetchMap,
  Sdk
> {
  /**
   * Creates an instance of the Application class.
   *
   * @param {SV} schemaValidator - The schema validator.
   */
  constructor(
    readonly schemaValidator: SV,
    readonly internal: Server,
    readonly postEnrichMiddleware: RouterHandler[],
    readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    readonly appOptions?: ExpressLikeApplicationOptions<SV, Session>
  ) {
    super(
      '/',
      schemaValidator,
      internal,
      postEnrichMiddleware,
      openTelemetryCollector,
      {
        ...appOptions,
        openapi: appOptions?.openapi !== false,
        mcp: appOptions?.mcp !== false
      }
    );

    // Cross-origin access is denied unless the app names its origins (the
    // `cors` option or CORS_ORIGINS); the cors package's own default is `*`.
    const { options: corsOptions, warning: corsWarning } = resolveCorsOptions(
      this.appOptions?.cors
    );
    if (corsWarning && !warnedAboutCorsDefaults) {
      warnedAboutCorsDefaults = true;
      this.openTelemetryCollector.warn(corsWarning);
    }
    this.internal.use(cors(corsOptions) as RouterHandler);
  }

  /**
   * Validates all registered routes across the app and all mounted routers.
   * Call this at the start of listen() to fail fast at startup.
   */
  protected validateAllRoutes() {
    this.validateSurfacingFunctions();
  }

  abstract listen(...args: unknown[]): void;
}
