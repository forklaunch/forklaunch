import { describe, expect, it } from 'vitest';
import { SchemaValidator, string } from '@forklaunch/validator/typebox';
import {
  ForklaunchExpressLikeRouter,
  MetricsDefinition,
  OpenTelemetryCollector
} from '../src/http';

function mockInternal() {
  return {
    use: () => {},
    get: () => {},
    post: () => {},
    put: () => {},
    delete: () => {},
    all: () => {},
    connect: () => {},
    patch: () => {},
    options: () => {},
    head: () => {},
    trace: () => {}
  };
}

function createRouter(
  basePath: `/${string}`,
  options?: Record<string, unknown>
) {
  return new ForklaunchExpressLikeRouter(
    basePath,
    SchemaValidator(),
    mockInternal(),
    [],
    {} as OpenTelemetryCollector<MetricsDefinition>,
    options as never
  );
}

describe('validateSurfacingFunctions', () => {
  it('passes when surfacePermissions is on the route', () => {
    const router = createRouter('/api');

    router.get(
      '/test',
      {
        name: 'WithSurface',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedPermissions: new Set(['read']),
          surfacePermissions: () => new Set(['read'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    expect(() => router.validateSurfacingFunctions()).not.toThrow();
  });

  it('passes when surfacePermissions is on the router options', () => {
    const router = createRouter('/api', {
      auth: {
        surfacePermissions: () => new Set(['read'])
      }
    });

    router.get(
      '/test',
      {
        name: 'InheritedSurface',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedPermissions: new Set(['read'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    expect(() => router.validateSurfacingFunctions()).not.toThrow();
  });

  it('throws when surfacePermissions is missing from route and router', () => {
    const router = createRouter('/api');

    router.get(
      '/test',
      {
        name: 'MissingSurface',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedPermissions: new Set(['read'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    expect(() => router.validateSurfacingFunctions()).toThrow(
      /Route 'MissingSurface'.*surfacePermissions.*route, router, or application/
    );
  });

  it('throws when surfaceRoles is missing', () => {
    const router = createRouter('/api');

    router.get(
      '/test',
      {
        name: 'MissingRoleSurface',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedRoles: new Set(['admin'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    expect(() => router.validateSurfacingFunctions()).toThrow(
      /Route 'MissingRoleSurface'.*surfaceRoles.*route, router, or application/
    );
  });

  it('throws when surfaceScopes is missing', () => {
    const router = createRouter('/api');

    router.get(
      '/test',
      {
        name: 'MissingScopeSurface',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          requiredScope: 'read:all'
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    expect(() => router.validateSurfacingFunctions()).toThrow(
      /Route 'MissingScopeSurface'.*surfaceScopes.*route, router, or application/
    );
  });

  it('passes for public routes without surfacing functions', () => {
    const router = createRouter('/api');

    router.get(
      '/health',
      {
        name: 'PublicRoute',
        access: 'public',
        summary: 'health check',
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    expect(() => router.validateSurfacingFunctions()).not.toThrow();
  });

  it('validates sub-router routes recursively', () => {
    const parent = createRouter('/api', {
      auth: {
        surfacePermissions: () => new Set(['read'])
      }
    });

    const child = createRouter('/child');

    child.get(
      '/test',
      {
        name: 'ChildRoute',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedPermissions: new Set(['read'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    // Before mounting, child has no surfacing functions
    expect(() => child.validateSurfacingFunctions()).toThrow(
      /Route 'ChildRoute'.*surfacePermissions/
    );

    // After mounting on parent, parent's options are merged into child
    parent.use(child);

    // Now validate from the parent — child has inherited options
    expect(() => parent.validateSurfacingFunctions()).not.toThrow();
  });

  it('propagates parent auth to deeply nested sub-routers composed before mounting', () => {
    const app = createRouter('/', {
      auth: {
        surfaceRoles: () => new Set(['admin'])
      }
    });

    const mid = createRouter('/mid');
    const leaf = createRouter('/leaf');

    leaf.get(
      '/test',
      {
        name: 'LeafRoute',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedRoles: new Set(['admin'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    // Compose child routers BEFORE mounting on app
    mid.use(leaf);
    app.use(mid);

    // App-level surfaceRoles should propagate to the leaf
    expect(() => app.validateSurfacingFunctions()).not.toThrow();
  });

  it('child router with own auth still inherits parent surfacing functions', () => {
    const parent = createRouter('/', {
      auth: {
        surfaceRoles: () => new Set(['admin'])
      }
    });

    const child = createRouter('/child', {
      auth: {
        sessionSchema: { name: 'string' }
      }
    });

    child.get(
      '/test',
      {
        name: 'ChildWithRoles',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedRoles: new Set(['admin'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    parent.use(child);

    // Parent's surfaceRoles should be visible even though child has its own auth
    expect(() => parent.validateSurfacingFunctions()).not.toThrow();
  });

  it('child router auth properties override parent for the same key', () => {
    const parentSurface = () => new Set(['parent-role']);
    const childSurface = () => new Set(['child-role']);

    const parent = createRouter('/', {
      auth: {
        surfaceRoles: parentSurface,
        surfacePermissions: () => new Set(['parent-perm'])
      }
    });

    const child = createRouter('/child', {
      auth: {
        surfaceRoles: childSurface
      }
    });

    child.get(
      '/test',
      {
        name: 'OverrideTest',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedRoles: new Set(['child-role']),
          allowedPermissions: new Set(['parent-perm'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    parent.use(child);

    // Child overrides surfaceRoles, parent's surfacePermissions fills the gap
    expect(() => parent.validateSurfacingFunctions()).not.toThrow();
  });

  it('sub-router auth overrides parent auth during validation', () => {
    const parent = createRouter('/', {
      auth: {
        surfaceRoles: () => new Set(['admin'])
      }
    });

    const child = createRouter('/child', {
      auth: {
        surfacePermissions: () => new Set(['read'])
      }
    });

    child.get(
      '/test',
      {
        name: 'ChildPermRoute',
        access: 'protected',
        summary: 'test',
        auth: {
          jwt: { signatureKey: 'secret' },
          allowedPermissions: new Set(['read'])
        },
        responses: { 200: string }
      },
      async (_req, res) => {
        res.status(200).send('ok');
      }
    );

    parent.use(child);

    // Child's own auth config should be used for its routes
    expect(() => parent.validateSurfacingFunctions()).not.toThrow();
  });
});
