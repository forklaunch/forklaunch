import { noop } from '@forklaunch/common';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { number, SchemaValidator, string } from '@forklaunch/validator/zod';
import {
  NextFunction as ExpressNextFunction,
  Request as ExpressRequest,
  Response as ExpressResponse
} from 'express';
import { Server } from 'http';
import { forklaunchExpress, forklaunchRouter } from '../index';
import { checkout } from '../src/handlers/checkout';
import { get } from '../src/handlers/get';
import { post } from '../src/handlers/post';

const expressMiddleware = (
  req: ExpressRequest,
  res: ExpressResponse,
  next: ExpressNextFunction
) => {
  noop(req, res, next);
  next();
};

const zodSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

const forklaunchApplication = forklaunchExpress(
  zodSchemaValidator,
  openTelemetryCollector
);
const forklaunchRouterInstance = forklaunchRouter(
  '/testpath',
  zodSchemaValidator,
  openTelemetryCollector
);
const nestedForklaunchRouterInstance = forklaunchRouter(
  '/nested',
  zodSchemaValidator,
  openTelemetryCollector
);

describe('Forklaunch Express Tests', () => {
  let server: Server;

  beforeAll(async () => {
    forklaunchRouterInstance.get(
      '/test',
      {
        name: 'Test',
        access: 'public',
        summary: 'Test Summary',
        responses: {
          200: string
        }
      },
      expressMiddleware,
      (_req, res) => {
        res.status(200).send('Hello World');
      }
    );

    forklaunchRouterInstance.post(
      '/test',
      {
        name: 'Test',
        access: 'public',
        summary: 'Test Summary',
        body: {
          test: string
        },
        responses: {
          200: string
        }
      },
      expressMiddleware,
      (req, res) => {
        res.status(200).send(req.body.test);
      }
    );

    forklaunchRouterInstance.put(
      '/test',
      {
        name: 'Test',
        access: 'public',
        summary: 'Test Summary',
        body: {
          test: string
        },
        responses: {
          200: string
        }
      },
      (req, res) => {
        res.status(200).send(req.body.test);
      }
    );

    forklaunchRouterInstance.patch(
      '/test',
      {
        name: 'Test',
        access: 'public',
        summary: 'Test Summary',
        body: {
          test: string
        },
        responses: {
          200: string
        }
      },
      (req, res) => {
        res.status(200).send(req.body.test);
      }
    );

    forklaunchRouterInstance.delete(
      '/test',
      {
        name: 'Test',
        access: 'public',
        summary: 'Test Summary',
        responses: {
          200: string
        }
      },
      (_req, res) => {
        res.status(200).send('Hello World');
      }
    );

    forklaunchApplication.use(forklaunchRouterInstance);

    server = await forklaunchApplication.listen(6840, () => {});
  });

  test('Get', async () => {
    const testGet = await fetch('http://localhost:6840/testpath/test', {
      method: 'GET'
    });

    expect(testGet.status).toBe(200);
    expect(await testGet.text()).toBe('Hello World');
  });

  test('Post', async () => {
    const testPost = await fetch('http://localhost:6840/testpath/test', {
      method: 'POST',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPost.status).toBe(200);
    expect(await testPost.text()).toBe('Hello World');
  });

  test('Put', async () => {
    const testPut = await fetch('http://localhost:6840/testpath/test', {
      method: 'PUT',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPut.status).toBe(200);
    expect(await testPut.text()).toBe('Hello World');
  });

  test('Patch', async () => {
    const testPatch = await fetch('http://localhost:6840/testpath/test', {
      method: 'PATCH',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPatch.status).toBe(200);
    expect(await testPatch.text()).toBe('Hello World');
  });

  test('Delete', async () => {
    const testDelete = await fetch('http://localhost:6840/testpath/test', {
      method: 'DELETE'
    });

    expect(testDelete.status).toBe(200);
    expect(await testDelete.text()).toBe('Hello World');
  });

  afterAll(async () => {
    server.close();
  });
});

describe('handlers', () => {
  const application = forklaunchExpress(
    zodSchemaValidator,
    openTelemetryCollector
  );
  const router = forklaunchRouter(
    '/organization',
    zodSchemaValidator,
    openTelemetryCollector
  );

  it('should be able to create a path param handler', async () => {
    const getRequest = get(
      zodSchemaValidator,
      '/:id',
      {
        name: 'Get Organization',
        access: 'protected',
        summary: 'Gets an organization by ID',
        responses: {
          200: {
            json: {
              ret: number
            }
            // text: string
          },
          404: string
        },
        params: {
          id: string
        },
        requestHeaders: {
          'x-test': string
        },
        auth: {
          jwt: {
            signatureKey: 'secret'
          },
          allowedRoles: new Set(['admin']),
          surfaceRoles: (_payload, _req) => {
            return new Set(['admin']);
          }
        }
      },
      async (req, res) => {
        const organizationDto = Number(req.params.id);
        if (organizationDto) {
          res.status(200).json({
            ret: organizationDto
          });
        } else {
          res.status(404).send('Organization not found');
        }
      }
    );
    application.get('/:id', getRequest);
    const liveTypeFunction = router.get('/:id', getRequest);

    await liveTypeFunction.fetch('/organization/:id', {
      method: 'GET',
      params: {
        id: 'string'
      },
      headers: {
        authorization: 'Bearer string',
        'x-test': 'string'
      }
    });
  });

  it('should be able to create a body param handler', async () => {
    const postRequest = post(
      zodSchemaValidator,
      '/',
      {
        name: 'Create Organization',
        access: 'public',
        summary: 'Creates an organization',
        responses: {
          200: {
            json: {
              name: string
            }
          }
        },
        body: {
          multipartForm: {
            name: string
          }
        }
      },
      async (req, res) => {
        res.status(200).json(req.body);
      }
    );

    const liveTypeFunction = router.post('/', postRequest);
    await liveTypeFunction.fetch('/organization', {
      method: 'POST',
      body: {
        multipartForm: {
          name: 'string'
        }
      }
    });
  });

  it('should be able to create a nested router', async () => {
    const postRequest = post(
      zodSchemaValidator,
      '/',
      {
        name: 'Create Organization',
        access: 'protected',
        body: {
          json: {
            name: string
          }
        },
        summary: 'Creates an organization',
        responses: {
          200: {
            json: {
              name: string
            }
          },
          400: string
        },
        auth: {
          jwt: {
            signatureKey: 'secret'
          },
          allowedPermissions: new Set(['admin']),
          surfacePermissions: (_payload, _req) => {
            return new Set(['admin']);
          }
        }
      },
      async (req, res) => {
        res.status(200).json(req.body);
      }
    );

    const liveTypeFunction = router.post('/', postRequest);

    const nestedLiveTypeFunction = nestedForklaunchRouterInstance
      .use(liveTypeFunction)
      .post('/', postRequest);

    await nestedLiveTypeFunction.fetch('/nested/organization', {
      method: 'POST',
      headers: {
        authorization: 'Bearer string'
      },
      body: {
        json: {
          name: 'string'
        }
      }
    });
  });

  it('should be able to create a middleware handler', () => {
    const checkoutMiddleware = checkout(
      zodSchemaValidator,
      '/',
      {
        query: {
          name: string
        }
      },
      async (req) => {
        if (req.query.name) {
          throw new Error('Name is required');
        }
      }
    );
    application.use(checkoutMiddleware);
    router.checkout('/', checkoutMiddleware);
  });
});
