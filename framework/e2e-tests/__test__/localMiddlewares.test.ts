import { OpenTelemetryCollector } from '@forklaunch/core/http';
import * as express from '@forklaunch/express';
import * as hyperExpress from '@forklaunch/hyper-express';
import * as typebox from '@forklaunch/validator/typebox';
import * as zod from '@forklaunch/validator/zod';
import { SignJWT } from 'jose';

// Local SDK calls (`router.sdk.x(...)`) skip middlewares unless the request
// sets `executeMiddlewares`; with it they must behave like a served request:
// authentication runs, a rejected request never reaches the controller, and
// an accepted one returns what the controller sent.

const signatureKey = 'local-middleware-secret';
const token = (roles: string[]) =>
  new SignJWT({ roles })
    .setProtectedHeader({ alg: 'HS256' })
    .setSubject('user-1')
    .setExpirationTime('5m')
    .sign(new TextEncoder().encode(signatureKey));

const frameworks = [
  ['express', express],
  ['hyper-express', hyperExpress]
] as const;
const validators = [
  ['zod', zod],
  ['typebox', typebox]
] as const;

for (const [name, framework] of frameworks)
  for (const [validatorName, validator] of validators) {
    describe(`local SDK calls with executeMiddlewares (${name}, ${validatorName})`, () => {
      // One set of types for every pairing; the runtime is each framework's
      // and validator's own.
      const forklaunchRouter =
        framework.forklaunchRouter as unknown as typeof express.forklaunchRouter;
      const schemaValidator =
        validator.SchemaValidator() as unknown as ReturnType<
          typeof zod.SchemaValidator
        >;
      const string = validator.string as unknown as typeof zod.string;
      const collector = new OpenTelemetryCollector('local-middlewares');
      let controllerRuns = 0;

      const router = forklaunchRouter('/local', schemaValidator, collector, {
        auth: {
          surfaceRoles: async (payload) =>
            new Set(
              Array.isArray(payload.roles) ? (payload.roles as string[]) : []
            )
        }
      });
      router.get(
        '/me',
        {
          name: 'Who Am I',
          summary: 'Returns the session subject',
          access: 'protected',
          auth: {
            jwt: { signatureKey },
            allowedRoles: new Set(['reader'])
          },
          responses: { 200: { sub: string } }
        },
        (req, res) => {
          controllerRuns++;
          res.status(200).json({ sub: String(req.session?.sub) });
        }
      );
      router.put(
        '/me',
        {
          name: 'Rename',
          summary: 'Echoes the new name for the session subject',
          access: 'protected',
          auth: {
            jwt: { signatureKey },
            allowedRoles: new Set(['reader'])
          },
          body: { name: string },
          responses: { 200: { sub: string, name: string } }
        },
        (req, res) => {
          controllerRuns++;
          res
            .status(200)
            .json({ sub: String(req.session?.sub), name: req.body.name });
        }
      );

      type LocalCall = (
        request: object
      ) => Promise<{ code: number; response: unknown }>;
      const sdk = router.sdk as unknown as {
        whoAmI: LocalCall;
        rename: LocalCall;
      };
      const call = (
        headers: Record<string, string>,
        executeMiddlewares = true
      ) => sdk.whoAmI({ headers, executeMiddlewares });

      beforeEach(() => {
        controllerRuns = 0;
      });

      it('returns what the controller sends for an authorized session', async () => {
        const response = await call({
          authorization: `Bearer ${await token(['reader'])}`
        });
        expect(response.code).toBe(200);
        expect(response.response).toEqual({ sub: 'user-1' });
        expect(controllerRuns).toBe(1);
      });

      it('answers 401 without a token and skips the controller', async () => {
        const response = await call({});
        expect(response.code).toBe(401);
        expect(controllerRuns).toBe(0);
      });

      it('answers 403 for a session without the role and skips the controller', async () => {
        const response = await call({
          authorization: `Bearer ${await token(['writer'])}`
        });
        expect(response.code).toBe(403);
        expect(controllerRuns).toBe(0);
      });

      it('parses the request body before the controller', async () => {
        const response = await sdk.rename({
          headers: { authorization: `Bearer ${await token(['reader'])}` },
          body: { name: 'Ada' },
          executeMiddlewares: true
        });
        expect(response.code).toBe(200);
        expect(response.response).toEqual({ sub: 'user-1', name: 'Ada' });
      });

      it('still skips middlewares by default', async () => {
        const response = await call({}, false);
        expect(response.code).toBe(200);
        expect(controllerRuns).toBe(1);
      });
    });
  }
