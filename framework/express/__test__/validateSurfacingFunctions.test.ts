import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/typebox';
import { Server } from 'http';
import { afterEach, describe, expect, it } from 'vitest';
import { forklaunchExpress, forklaunchRouter } from '../index';

const sv = SchemaValidator();
const otel = new OpenTelemetryCollector('test');

describe('validateSurfacingFunctions at listen()', () => {
  let server: Server | undefined;

  afterEach(() => {
    server?.close();
    server = undefined;
  });

  it('throws at listen() when router has permissions but no surfacePermissions anywhere', async () => {
    const app = forklaunchExpress(sv, otel);
    const router = forklaunchRouter('/api', sv, otel);

    router.get(
      '/protected',
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

    app.use(router);

    expect(() => app.listen(0)).toThrow(
      /Route 'MissingSurface'.*surfacePermissions.*route, router, or application/
    );
  });

  it('throws at listen() when router has roles but no surfaceRoles anywhere', async () => {
    const app = forklaunchExpress(sv, otel);
    const router = forklaunchRouter('/api', sv, otel);

    router.get(
      '/protected',
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

    app.use(router);

    expect(() => app.listen(0)).toThrow(
      /Route 'MissingRoleSurface'.*surfaceRoles.*route, router, or application/
    );
  });

  it('passes when surfacePermissions is provided at the app level', async () => {
    const app = forklaunchExpress(sv, otel, {
      auth: {
        surfacePermissions: () => new Set(['read'])
      }
    });
    const router = forklaunchRouter('/api', sv, otel);

    router.get(
      '/protected',
      {
        name: 'AppLevelSurface',
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

    app.use(router);

    server = app.listen(0) as Server;
    expect(server).toBeDefined();
  });

  it('passes when surfacePermissions is on the router, not the app', async () => {
    const app = forklaunchExpress(sv, otel);
    const router = forklaunchRouter('/api', sv, otel, {
      auth: {
        surfacePermissions: () => new Set(['read'])
      }
    });

    router.get(
      '/protected',
      {
        name: 'RouterLevelSurface',
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

    app.use(router);

    server = app.listen(0) as Server;
    expect(server).toBeDefined();
  });

  it('passes when surfacePermissions is on the route itself', async () => {
    const app = forklaunchExpress(sv, otel);
    const router = forklaunchRouter('/api', sv, otel);

    router.get(
      '/protected',
      {
        name: 'RouteLevelSurface',
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

    app.use(router);

    server = app.listen(0) as Server;
    expect(server).toBeDefined();
  });

  it('passes when surfaceRoles is on the app and sub-router is nested before mounting', async () => {
    const app = forklaunchExpress(sv, otel, {
      auth: {
        surfaceRoles: () => new Set(['admin'])
      }
    });
    const mid = forklaunchRouter('/mid', sv, otel);
    const leaf = forklaunchRouter('/leaf', sv, otel);

    leaf.get(
      '/protected',
      {
        name: 'DeepRoute',
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

    // Compose before mounting on app
    mid.use(leaf);
    app.use(mid);

    server = app.listen(0) as Server;
    expect(server).toBeDefined();
  });

  it('propagates app-level auth through a nested app that has no auth of its own', async () => {
    const outer = forklaunchExpress(sv, otel, {
      auth: {
        surfaceRoles: () => new Set(['admin'])
      }
    });
    const inner = forklaunchExpress(sv, otel);
    const router = forklaunchRouter('/api', sv, otel);

    router.get(
      '/protected',
      {
        name: 'NestedAppRoute',
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

    inner.use(router);
    outer.use(inner);

    server = outer.listen(0) as Server;
    expect(server).toBeDefined();
  });

  it('passes for public routes without any surfacing functions', async () => {
    const app = forklaunchExpress(sv, otel);
    const router = forklaunchRouter('/api', sv, otel);

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

    app.use(router);

    server = app.listen(0) as Server;
    expect(server).toBeDefined();
  });
});
