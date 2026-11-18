---
'@forklaunch/core': major
'@forklaunch/express': major
'@forklaunch/hyper-express': major
---

BREAKING: cross-origin browser access is now denied unless the application names its origins. An application created without a `cors` option used to answer every origin with `Access-Control-Allow-Origin: *`; it now allows only the origins in the comma separated `CORS_ORIGINS` environment variable, and when that is unset it allows no origin when `NODE_ENV=production` and only `localhost` / `127.0.0.1` / `[::1]` otherwise (one startup warning explains this). Who is affected: any service with a browser frontend on a different origin that passes no `cors` option and has no `CORS_ORIGINS` set. Fix: set `CORS_ORIGINS=https://your-ui` (several: `CORS_ORIGINS=https://app.example.com,https://admin.example.com`), or pass `cors: { origin: ['https://your-ui'] }` in the application options. Server-to-server, curl and same-origin callers are unaffected. An explicit `cors` option is still used exactly as given, which means an object without `origin`, including an empty `cors: {}`, still allows every origin; a startup warning now says so. `Access-Control-Allow-Credentials: true` is never sent together with a wildcard origin (browsers already rejected that pair): name the origins to use credentials.
