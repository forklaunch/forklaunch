import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { number, SchemaValidator, string } from '@forklaunch/validator/typebox';
import { Server } from 'http';
import { forklaunchExpress, forklaunchRouter } from '../index';
import { checkout } from '../src/handlers/checkout';
import { get } from '../src/handlers/get';
import { post } from '../src/handlers/post';

import { noop, safeStringify } from '@forklaunch/common';
import {
  NextFunction as ExpressNextFunction,
  Request as ExpressRequest,
  Response as ExpressResponse
} from 'express';

const expressMiddleware = (
  req: ExpressRequest,
  res: ExpressResponse,
  next: ExpressNextFunction
) => {
  noop(req, res, next);
  next();
};

const typeboxSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

const forklaunchApplication = forklaunchExpress(
  typeboxSchemaValidator,
  openTelemetryCollector
);
const forklaunchRouterInstance = forklaunchRouter(
  '/testpath',
  typeboxSchemaValidator,
  openTelemetryCollector
);

const nestedForklaunchRouterInstance = forklaunchRouter(
  '/nested',
  typeboxSchemaValidator,
  openTelemetryCollector
);

describe('Forklaunch Express Tests', () => {
  let server: Server;

  beforeAll(async () => {
    forklaunchRouterInstance.get(
      '/test',
      {
        name: 'Test',
        summary: 'Test Summary',
        access: 'public',
        responses: {
          200: string
        }
      },
      expressMiddleware,
      async (_req, res) => {
        res.status(200).send('Hello World');
      }
    );

    forklaunchRouterInstance.post(
      '/test',
      {
        name: 'Test',
        summary: 'Test Summary',
        access: 'public',
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
        summary: 'Test Summary',
        access: 'public',
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
        summary: 'Test Summary',
        access: 'public',
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
        summary: 'Test Summary',
        access: 'public',
        responses: {
          200: string
        }
      },
      (_req, res) => {
        res.status(200).send('Hello World');
      }
    );

    forklaunchApplication.use(forklaunchRouterInstance);

    server = await forklaunchApplication.listen(6934, () => {});
  });

  test('Get', async () => {
    const testGet = await fetch('http://localhost:6934/testpath/test', {
      method: 'GET'
    });

    expect(testGet.status).toBe(200);
    expect(await testGet.text()).toBe('Hello World');
  });

  test('Post', async () => {
    const testPost = await fetch('http://localhost:6934/testpath/test', {
      method: 'POST',
      body: safeStringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPost.status).toBe(200);
    expect(await testPost.text()).toBe('Hello World');
  });

  test('Put', async () => {
    const testPut = await fetch('http://localhost:6934/testpath/test', {
      method: 'PUT',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPut.status).toBe(200);
    expect(await testPut.text()).toBe('Hello World');
  });

  test('Patch', async () => {
    const testPatch = await fetch('http://localhost:6934/testpath/test', {
      method: 'PATCH',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPatch.status).toBe(200);
    expect(await testPatch.text()).toBe('Hello World');
  });

  test('Delete', async () => {
    const testDelete = await fetch('http://localhost:6934/testpath/test', {
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
    typeboxSchemaValidator,
    openTelemetryCollector
  );
  const router = forklaunchRouter(
    '/organization',
    typeboxSchemaValidator,
    openTelemetryCollector
  );

  it('should be able to create a path param handler', async () => {
    const getRequest = get(
      typeboxSchemaValidator,
      '/:id',
      {
        name: 'Get Organization',
        summary: 'Gets an organization by ID',
        access: 'protected',
        responses: {
          200: number,
          404: string
        },
        params: {
          id: string
        },
        requestHeaders: {
          'x-test': string
        },
        auth: {
          sessionSchema: {
            organizationId: string
          },
          tokenPrefix: 'Bearerz',
          basic: {
            login: (username: string, password: string) => {
              return username === 'test' && password === 'test';
            }
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
          res.status(200).json(organizationDto);
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
        authorization: 'Bearerz string',
        'x-test': 'string'
      }
    });
  });

  it('should be able to create a body param handler', async () => {
    const postRequest = post(
      typeboxSchemaValidator,
      '/',
      {
        name: 'Create Organization',
        access: 'public',
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
        json: {
          name: 'string'
        }
      }
    });
  });

  it('should be able to create a nested router', async () => {
    const postRequest = post(
      typeboxSchemaValidator,
      '/',
      {
        name: 'Create Organization',
        access: 'public',
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
      body: {
        json: {
          name: 'string'
        }
      }
    });
  });

  it('should be able to create a middleware handler', () => {
    const checkoutMiddleware = checkout(
      typeboxSchemaValidator,
      '/',
      {
        query: {
          name: string
        }
      },
      async (req) => {
        noop(req.query.name);
      }
    );
    application.use(checkoutMiddleware);
    router.checkout('/', checkoutMiddleware);
  });
});
