import { forklaunchExpress, schemaValidator } from '@forklaunch/blueprint-core';
import {
  betterAuthTelemetryHookMiddleware,
  enrichBetterAuthApi
} from './api/middlewares/betterAuth.middleware';
import { discoveryRouter } from './api/routes/discovery.routes';
import { userRouter } from './api/routes/user.routes';
import { BetterAuth } from './auth';
import { ci, tokens } from './bootstrapper';
import { iamSdkClient } from './sdk';

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(
  schemaValidator,
  openTelemetryCollector,
  await ci.resolve(tokens.ExpressApplicationOptions)
);

//! Cookie-less test-auth callback: extracts tokens from the session cookie set by
//! better-auth during OAuth/magic-link, clears the cookie, and passes tokens via
//! URL hash to the final callback page. This prevents the test flow from overwriting
//! the main app's session cookie.
app.internal.get('/api/auth/test-callback', async (req, res) => {
  const callbackUrl = String(req.query.callbackUrl || '');
  if (!callbackUrl) {
    res.status(400).send('Missing callbackUrl');
    return;
  }

  const cookie = req.headers.cookie || '';
  const origin = `${req.protocol}://${req.headers.host}`;

  try {
    const [tokenRes, sessionRes] = await Promise.all([
      fetch(`${origin}/api/auth/token`, { headers: { cookie } }),
      fetch(`${origin}/api/auth/get-session`, { headers: { cookie } })
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

    res.setHeader(
      'Set-Cookie',
      'better-auth.session_token=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax'
    );

    const hash = new URLSearchParams({ token, sessionToken, email }).toString();
    res.redirect(`${callbackUrl}#${hash}`);
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
app.internal.get('/api/auth/oauth-redirect', (req, res) => {
  const provider = String(req.query.provider || '');
  const callbackURL = String(req.query.callbackURL || '');
  const organizationId = String(req.query.organizationId || '');
  const endpoint =
    req.query.endpoint === 'sso'
      ? '/api/auth/sign-in/sso'
      : '/api/auth/sign-in/social';

  const origin = `${req.protocol}://${req.headers.host}`;
  const intermediateCallbackURL = `${origin}/api/auth/test-callback?callbackUrl=${encodeURIComponent(callbackURL)}`;

  const body =
    req.query.endpoint === 'sso'
      ? { organizationId, callbackURL: intermediateCallbackURL }
      : { provider, callbackURL: intermediateCallbackURL };
  res.type('html').send(`<!DOCTYPE html><html><body><p>Redirecting…</p><script>
    fetch(${JSON.stringify(endpoint)}, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: ${JSON.stringify(JSON.stringify(body))},
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

//! register the sdk client
app.registerSdks(iamSdkClient);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 IAM Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});
