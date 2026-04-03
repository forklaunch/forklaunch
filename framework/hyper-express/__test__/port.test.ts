import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/typebox';
import { afterAll, beforeAll, describe, expect, test } from 'vitest';
import { forklaunchExpress, port } from '../index';

const typeboxSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

describe('port function integration', () => {
  const app = forklaunchExpress(typeboxSchemaValidator, openTelemetryCollector);
  const portNum = 6937; // Unique port for this test

  beforeAll(async () => {
    // defined handlers using port()
    const handlers = port(
      {
        method: 'get',
        path: '/port-test',
        basePath: '',
        schemaValidator: typeboxSchemaValidator,
        openTelemetryCollector: openTelemetryCollector
      },
      {
        name: 'TestPort',
        access: 'public',
        summary: 'Test Port Function',
        responses: {
          200: string
        }
      },
      async (_req, res) => {
        res.status(200).send('Hello Port');
      }
    );

    // Mount handlers to the app
    // In hyper-express, handlers from port() are MiddlewareHandlers (params: req, res, next)
    // We can use them directly with app.use or specific method if adapted,
    // but port() returns MiddlewareHandler[], and app.get/use expects standard handlers.
    // However, forklaunchExpress returns a wrapper Application.
    // Let's see how `port` results are intended to be used.
    // In Express `port` returns `RequestHandler[]`.
    // In Hyper-Express `port` returns `MiddlewareHandler[]`.

    // HyperExpress underlying instance is `app.internal`.
    // But `port` handlers are designed to be compatible with the framework.
    // Let's verify how they are used.
    // `port` returns an array of handlers.
    // We should be able to spread them into a route definition on the raw instance or use them with the wrapper if supported?
    // The `Application` wrapper in `src/hyperExpressApplication.ts` might not have a direct method to accept an array for a path blindly like express `app.get(path, ...handlers)`.
    // Wait, the `Application` wrapper methods (get, post, etc.) take a `TypedHandler`.
    // If we want to use `port` output (which is raw handlers), we might need to use the underlying instance or `app.use`.

    // Actually, looking at `framework/express/index.ts`, `port` returns `RequestHandler[]` which are standard express handlers.
    // In `framework/hyper-express/index.ts`, `port` returns `MiddlewareHandler[]`.

    // Let's use the underlying hyper-express instance to register these for the test,
    // as `port` is intended to generate raw handlers compatible with the framework.
    // `app.internal` gives access to the underlying HyperExpress.Server instance.

    // Register the route once with all handlers
    app.internal.get('/port-test', ...handlers);

    await app.listen(portNum);
  });

  afterAll(async () => {
    app.internal.close();
  });

  test('should successfully handle request on route registered via port()', async () => {
    const response = await fetch(`http://localhost:${portNum}/port-test`, {
      method: 'GET'
    });

    expect(response.status).toBe(200);
    expect(await response.text()).toBe('Hello Port');
  });
});
