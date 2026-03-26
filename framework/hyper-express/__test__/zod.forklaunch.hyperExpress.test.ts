/* eslint-disable @typescript-eslint/no-unused-vars */
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { number, SchemaValidator, string } from '@forklaunch/validator/zod';
import { forklaunchExpress, forklaunchRouter } from '../index';
import { any } from '../src/handlers/any';
import { get } from '../src/handlers/get';
import { post } from '../src/handlers/post';

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

// forklaunchRouterInstance.upgrade({
//   'upgradeSchema': {
//     x: string
//   }
// }, (req, res) => {
//   res.upgrade()
// })
describe('Forklaunch Hyper-Express Tests', () => {
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

    await forklaunchApplication.listen(6937, () => {});
  });

  test('Get', async () => {
    const testGet = await fetch('http://localhost:6937/testpath/test', {
      method: 'GET'
    });

    expect(testGet.status).toBe(200);
    expect(await testGet.text()).toBe('Hello World');
  });

  test('Post', async () => {
    const testPost = await fetch('http://localhost:6937/testpath/test', {
      method: 'POST',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPost.status).toBe(200);
    expect(await testPost.text()).toBe('Hello World');
  });

  test('Put', async () => {
    const testPut = await fetch('http://localhost:6937/testpath/test', {
      method: 'PUT',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPut.status).toBe(200);
    expect(await testPut.text()).toBe('Hello World');
  });

  test('Patch', async () => {
    const testPatch = await fetch('http://localhost:6937/testpath/test', {
      method: 'PATCH',
      body: JSON.stringify({ test: 'Hello World' }),
      headers: { 'Content-Type': 'application/json' }
    });

    expect(testPatch.status).toBe(200);
    expect(await testPatch.text()).toBe('Hello World');
  });

  test('Delete', async () => {
    const testDelete = await fetch('http://localhost:6937/testpath/test', {
      method: 'DELETE'
    });

    expect(testDelete.status).toBe(200);
    expect(await testDelete.text()).toBe('Hello World');
  });

  afterAll(async () => {
    forklaunchApplication.internal.shutdown();
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

  it('should be able to create a path param handler', () => {
    const getRequest = get(
      zodSchemaValidator,
      '/:id',
      {
        name: 'Get Organization',
        access: 'protected',
        summary: 'Gets an organization by ID',
        responses: {
          200: {
            ret: number
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
            jwksPublicKey: {
              kty: 'RSA',
              e: 'AQAB',
              n: 'AQAB',
              kid: '1234567890'
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
          res.status(200).json({
            ret: organizationDto
          });
        } else {
          res.status(404).send('Organization not found');
        }
      }
    );
    application.get('/:id', getRequest);
    router.get('/:id', getRequest);
  });

  it('should be able to create a body param handler', () => {
    const postRequest = post(
      zodSchemaValidator,
      '/',
      {
        name: 'Create Organization',
        access: 'public',
        summary: 'Creates an organization',
        responses: {
          200: {
            name: string
          }
        },
        body: {
          name: string
        }
      },
      async (req, res) => {
        res.status(200).json(req.body);
      }
    );

    router.post('/', postRequest);
  });

  it('should be able to create a nested router', async () => {
    const postRequest = post(
      zodSchemaValidator,
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
    const checkoutMiddleware = any(
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
    router.any('/', checkoutMiddleware);
    router.upgrade(
      '/',
      {
        upgrade: {
          a: string
        }
      },
      (req, res) => {
        res.upgrade({
          a: 'fff'
        });
      }
    );
  });
});
