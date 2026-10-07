import {
  MiddlewareHandler,
  MiddlewareNext,
  Request,
  Response
} from '@forklaunch/hyper-express-fork';
import LiveDirectory from 'live-directory';
import { OpenAPIObject } from 'openapi3-ts/oas31';
import getAbsoluteSwaggerFsPath from 'swagger-ui-dist/absolute-path';
import swaggerUi from 'swagger-ui-express';

/**
 * Middleware to redirect requests to the Swagger UI base path.
 * Ensures that requests to the base path are redirected to include a trailing slash,
 * which is required for proper Swagger UI functionality.
 *
 * @param {string} path - The base path for the Swagger UI (e.g., '/api/v1/docs')
 * @returns {MiddlewareHandler} The middleware handler for redirecting requests
 *
 * @example
 * ```typescript
 * const swaggerPath = '/api/v1/docs';
 * app.use(swaggerPath, swaggerRedirect(swaggerPath));
 * ```
 */
export function swaggerRedirect(path: string): MiddlewareHandler {
  return function swaggerHosting(
    req: Request,
    res: Response,
    next?: MiddlewareNext
  ) {
    if (req.path === path) {
      res.redirect(`${path}/`);
    }
    return next?.();
  };
}

/**
 * Sets up the Swagger UI middleware for serving API documentation.
 * This function configures and returns middleware handlers that serve the Swagger UI
 * interface and its associated assets. It uses live-directory for efficient asset
 * serving and caching.
 *
 * @param {string} path - The base path for the Swagger UI (e.g., '/api/v1/docs')
 * @param {OpenAPIObject} document - The OpenAPI specification document to display
 * @param {swaggerUi.SwaggerUiOptions} [opts] - Optional Swagger UI configuration options
 * @param {swaggerUi.SwaggerOptions} [options] - Optional Swagger specification display options
 * @param {string} [customCss] - Custom CSS to apply to the Swagger UI interface
 * @param {string} [customfavIcon] - Custom favicon URL for the Swagger UI
 * @param {string} [swaggerUrl] - Custom URL for loading the Swagger specification
 * @param {string} [customSiteTitle] - Custom title for the Swagger UI page
 * @returns {MiddlewareHandler[]} Array of middleware handlers: [serve, staticAssets, ui]
 *
 * @example
 * ```typescript
 * const swaggerPath = '/api/v1/docs';
 * const swaggerSpec = generateOpenApiSpecs(schemaValidator, 3000, routers);
 *
 * app.get(
 *   `${swaggerPath}/*`,
 *   swagger(
 *     swaggerPath,
 *     swaggerSpec,
 *     { explorer: true },
 *     undefined,
 *     '.swagger-ui .topbar { background-color: #24292e; }'
 *   )
 * );
 * ```
 */
export function swagger(
  path: string,
  document: OpenAPIObject,
  opts?: swaggerUi.SwaggerUiOptions,
  options?: swaggerUi.SwaggerOptions,
  customCss?: string,
  customfavIcon?: string,
  swaggerUrl?: string,
  customSiteTitle?: string
): MiddlewareHandler[] {
  const LiveAssets = new LiveDirectory(getAbsoluteSwaggerFsPath(), {
    filter: {
      keep: {
        names: [
          'swagger-ui-bundle.js',
          'swagger-ui-standalone-preset.js',
          'swagger-ui-init.js',
          'swagger-ui.css',
          'favicon-32x32.png',
          'favicon-16x16.png'
        ]
      }
    },
    cache: {
      max_file_count: 10,
      max_file_size: 1024 * 1024 * 1.5
    }
  });

  const serve = swaggerUi.serve[0] as unknown as MiddlewareHandler;

  /**
   * Middleware to serve static assets for the Swagger UI.
   * Handles serving of JavaScript, CSS, and image files required by the Swagger UI.
   * Uses live-directory for efficient caching and serving of assets.
   *
   * @param {Request} req - The Hyper-Express request object
   * @param {Response} res - The Hyper-Express response object
   * @param {MiddlewareNext} [next] - The next middleware function
   * @returns {void}
   */
  const staticAssets = (req: Request, res: Response, next?: MiddlewareNext) => {
    const filePath = req.path.replace(path, '');
    const file = LiveAssets.get(filePath);

    if (file === undefined) {
      if (next) {
        return next();
      }
      return res.status(404).send();
    }

    const fileParts = file.path.split('.');
    const extension = fileParts[fileParts.length - 1];

    const content = file.content;
    return res.type(extension).send(content);
  };

  const ui = swaggerUi.setup(
    document,
    opts,
    options,
    customCss,
    customfavIcon,
    swaggerUrl,
    customSiteTitle
    // swagger-ui-express is typed for Express' `RequestHandler`. hyper-express
    // calls handlers with its own request/response classes, so the signatures do
    // not overlap even though the handler runs unchanged under both.
  ) as unknown as MiddlewareHandler;

  return [serve, staticAssets, ui];
}
