import { getEnvVar } from '@forklaunch/common';
import { betterAuthSurface } from './api/contracts/auth.surface';
import {
  forklaunchExpress,
  registerDelegatedSurface,
  schemaValidator
} from '@forklaunch/blueprint-core';
import { setupRls, setupTenantFilter } from '@forklaunch/core/persistence';
import {
  betterAuthTelemetryHookMiddleware,
  enrichBetterAuthApi
} from './api/middlewares/betterAuth.middleware';
import { discoveryRouter } from './api/routes/discovery.routes';
import { userRouter } from './api/routes/user.routes';
import { complianceRouter } from './api/routes/compliance.routes';
import { BetterAuth } from './auth';
import { ci, tokens } from './bootstrapper';
import { iamSdkClient } from './sdk';
import {
  scriptJson,
  trustedCallbackUrl
} from './domain/utils/betterAuthRedirect.util';

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

//! Query values may arrive as arrays or nested objects; only a plain string is
//! a usable callback, provider or organization value.
const queryString = (value: unknown): string =>
  typeof value === 'string' ? value : '';
const orm = ci.resolve(tokens.Orm);
setupTenantFilter(orm, { logger: openTelemetryCollector });
setupRls(orm, { logger: openTelemetryCollector });

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(
  schemaValidator,
  openTelemetryCollector,
  await ci.resolve(tokens.ExpressApplicationOptions)
);

//! Use the resolved URL: Better Auth lets a base URL path override basePath.
const resolvedAuth = ci.resolve(tokens.BetterAuth) as BetterAuth;
const resolvedAuthContext = await resolvedAuth.$context;
const authBaseURL = `${resolvedAuthContext.baseURL}`.replace(/\/+$/, '');
//! Contract export (FORKLAUNCH_MODE=openapi) resolves services to inert
//! stand-ins with no URL; describe the configured base path there.
const authBasePath = (
  URL.canParse(authBaseURL)
    ? new URL(authBaseURL).pathname
    : (getEnvVar('BETTER_AUTH_BASE_PATH') ?? '/api/auth')
).replace(/\/+$/, '');

registerDelegatedSurface(
  betterAuthSurface,
  {
    basePath: authBasePath,
    paths: authBasePath
      ? `${authBasePath}/{*any}`
      : [
          ...new Set(
            Object.values(resolvedAuth.api)
              .map((endpoint) => (endpoint as { path?: string }).path)
              .filter((path): path is string => Boolean(path))
              .map((path) => `/${path.split('/')[1]}{/{*any}}`)
          )
        ]
  },
  (mount) => {
    //! Cookie-less test-auth callback: extracts tokens from the session cookie set by
    //! better-auth during OAuth/magic-link, clears the cookie, and passes tokens via
    //! URL hash to the final callback page. This prevents the test flow from overwriting
    //! the main app's session cookie.
    app.internal.get(`${mount.basePath}/test-callback`, async (req, res) => {
      const callbackUrl = queryString(req.query.callbackUrl);
      if (!callbackUrl) {
        res.status(400).send('Missing callbackUrl');
        return;
      }

      const auth = ci.resolve(tokens.BetterAuth) as BetterAuth;
      const authContext = await auth.$context;
      const callback = trustedCallbackUrl(callbackUrl, authContext);
      if (!callback) {
        res.status(400).send('Invalid callbackUrl');
        return;
      }
      const cookie = req.headers.cookie || '';

      try {
        const [tokenRes, sessionRes] = await Promise.all([
          fetch(`${authBaseURL}/token`, { headers: { cookie } }),
          fetch(`${authBaseURL}/get-session`, { headers: { cookie } })
        ]);

        let token = '',
          sessionToken = '',
          email = 'unknown';
        if (tokenRes.ok) {
          const data = (await tokenRes.json()) as { token?: string };
          token = data.token || '';
        }
        if (sessionRes.ok) {
          const data = (await sessionRes.json()) as {
            user?: { email?: string };
            session?: { token?: string };
          };
          email = data?.user?.email || 'unknown';
          sessionToken = data?.session?.token || '';
        }

        const sessionCookie = authContext.authCookies.sessionToken;
        const cookiePath = sessionCookie.attributes.path ?? '/';
        const cookieDomain = sessionCookie.attributes.domain;
        res.setHeader(
          'Set-Cookie',
          `${sessionCookie.name}=; Path=${cookiePath}; Max-Age=0; HttpOnly; SameSite=Lax` +
            (sessionCookie.attributes.secure ? '; Secure' : '') +
            (cookieDomain ? `; Domain=${cookieDomain}` : '')
        );

        const hash = new URLSearchParams({
          token,
          sessionToken,
          email
        }).toString();
        callback.hash = hash;
        res.redirect(callback.href);
      } catch (err) {
        res
          .status(500)
          .send(
            'Failed to exchange session: ' +
              (err instanceof Error ? err.message : String(err))
          );
      }
    });

    //! serves a redirect page for OAuth popup flows (must be before the catch-all)
    app.internal.get(`${mount.basePath}/oauth-redirect`, async (req, res) => {
      const provider = queryString(req.query.provider);
      const callbackURL = queryString(req.query.callbackURL);
      const authContext = await (ci.resolve(tokens.BetterAuth) as BetterAuth)
        .$context;
      const callback = trustedCallbackUrl(callbackURL, authContext);
      if (!callbackURL || !callback) {
        res.status(400).send('Invalid callbackURL');
        return;
      }
      const organizationId = queryString(req.query.organizationId);
      const endpoint =
        req.query.endpoint === 'sso'
          ? `${authBasePath}/sign-in/sso`
          : `${authBasePath}/sign-in/social`;

      const intermediateCallbackURL = `${authBaseURL}/test-callback?callbackUrl=${encodeURIComponent(callback.href)}`;

      const body =
        req.query.endpoint === 'sso'
          ? { organizationId, callbackURL: intermediateCallbackURL }
          : { provider, callbackURL: intermediateCallbackURL };
      res.type('html')
        .send(`<!DOCTYPE html><html><body><p>Redirecting…</p><script>
    fetch(${scriptJson(endpoint)}, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: ${scriptJson(JSON.stringify(body))},
      credentials: 'include',
    })
    .then(function(r) { return r.json(); })
    .then(function(data) {
      if (data.url) window.location.href = data.url;
      else document.body.innerText = 'Error: ' + JSON.stringify(data);
    })
    .catch(function(e) { document.body.innerText = 'Error: ' + e.message; });
  </script></body></html>`);
    });

    //! registers the betterAuth middleware
    // One registration per path: Express accepts a path array, hyper-express
    // takes a single pattern.
    const betterAuthHandler = enrichBetterAuthApi(
      ci.resolve(tokens.BetterAuth) as BetterAuth
    );
    for (const path of Array.isArray(mount.paths)
      ? mount.paths
      : [mount.paths]) {
      app.internal.all(
        path,
        betterAuthTelemetryHookMiddleware,
        betterAuthHandler
      );
    }
  }
);

//! resolves the host, port, and version from the configuration
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(discoveryRouter);
app.use(userRouter);
app.use(complianceRouter);

//! register the sdk client
app.registerSdks(iamSdkClient);

//! starts the server
// hyper-express returns a Promise from listen; nothing awaits startup here.
void app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 IAM Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});
