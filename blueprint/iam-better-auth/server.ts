import {
  trustedAuthCallback,
  configuredAuthOrigin,
  inlineScriptJson
} from './domain/utils/authRedirect.util';
import { forklaunchExpress, schemaValidator } from '@forklaunch/blueprint-core';
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

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const orm = ci.resolve(tokens.Orm);
setupTenantFilter(orm, { logger: openTelemetryCollector });
setupRls(orm, { logger: openTelemetryCollector });

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(
  schemaValidator,
  openTelemetryCollector,
  await ci.resolve(tokens.ExpressApplicationOptions)
);

const betterAuth = ci.resolve(tokens.BetterAuth) as BetterAuth;

//! Cookie-less test-auth callback: extracts tokens from the session cookie set by
//! better-auth during OAuth/magic-link, clears the cookie, and passes tokens via
//! URL hash to the final callback page. This prevents the test flow from overwriting
//! the main app's session cookie.
app.internal.get('/api/auth/test-callback', async (req, res) => {
  const callbackUrl = trustedAuthCallback(
    req.query.callbackUrl,
    ci.resolve(tokens.CORS_ORIGINS) ?? []
  );
  if (!callbackUrl) {
    res.status(400).send('Invalid callback URL');
    return;
  }

  const cookie = req.headers.cookie || '';
  const origin = configuredAuthOrigin(betterAuth.options.baseURL);

  try {
    const [tokenRes, sessionRes] = await Promise.all([
      fetch(`${origin}/api/auth/token`, {
        headers: { cookie },
        redirect: 'error',
        signal: AbortSignal.timeout(15000)
      }),
      fetch(`${origin}/api/auth/get-session`, {
        headers: { cookie },
        redirect: 'error',
        signal: AbortSignal.timeout(15000)
      })
    ]);

    if (!tokenRes.ok || !sessionRes.ok) {
      res.status(401).send('Sign in again to continue.');
      return;
    }

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

    res.setHeader(
      'Set-Cookie',
      'better-auth.session_token=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax'
    );

    const hash = new URLSearchParams({ token, sessionToken, email }).toString();
    callbackUrl.hash = hash;
    res.setHeader('Cache-Control', 'no-store');
    res.setHeader('Referrer-Policy', 'no-referrer');
    res.redirect(callbackUrl.toString());
  } catch {
    res.status(500).send('Unable to complete sign-in. Please try again.');
  }
});

//! serves a redirect page for OAuth popup flows (must be before the catch-all)
app.internal.get('/api/auth/oauth-redirect', (req, res) => {
  const provider = String(req.query.provider || '');
  const callback = trustedAuthCallback(
    req.query.callbackURL,
    ci.resolve(tokens.CORS_ORIGINS) ?? []
  );
  if (!callback) {
    res.status(400).send('Invalid callback URL');
    return;
  }
  const callbackURL = callback.toString();
  const organizationId = String(req.query.organizationId || '');
  const endpoint =
    req.query.endpoint === 'sso'
      ? '/api/auth/sign-in/sso'
      : '/api/auth/sign-in/social';

  const origin = configuredAuthOrigin(betterAuth.options.baseURL);
  const intermediateCallbackURL = `${origin}/api/auth/test-callback?callbackUrl=${encodeURIComponent(callbackURL)}`;

  const body =
    req.query.endpoint === 'sso'
      ? { organizationId, callbackURL: intermediateCallbackURL }
      : { provider, callbackURL: intermediateCallbackURL };
  res.type('html').send(`<!DOCTYPE html><html><body><p>Redirecting…</p><script>
    fetch(${inlineScriptJson(endpoint)}, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: ${inlineScriptJson(JSON.stringify(body))},
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
app.internal.all(
  '/api/auth/{*any}',
  betterAuthTelemetryHookMiddleware,
  enrichBetterAuthApi(ci.resolve(tokens.BetterAuth) as BetterAuth)
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
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 IAM Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});
