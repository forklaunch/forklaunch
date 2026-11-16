import corsMiddleware, { CorsOptions } from 'cors';
import { ParsedQs } from 'qs';
import {
  ForklaunchBaseRequest,
  ForklaunchNextFunction,
  ForklaunchResHeaders,
  ForklaunchResponse
} from '../../types/apiDefinition.types';
import { ParamsDictionary } from '../../types/contractDetails.types';

/** The environment variable holding the allowed browser origins. */
export const CORS_ORIGINS_ENV = 'CORS_ORIGINS';

/**
 * Origins allowed outside production when nothing is configured: this
 * machine only (`localhost`, `127.0.0.1`, `[::1]`), any port, http or https.
 * Anchored, so `http://localhost.evil.example` does not match.
 */
export const LOCAL_DEVELOPMENT_ORIGIN =
  /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d{1,5})?$/;

/** Parse a comma separated origin list; trailing slashes are dropped. */
export function parseCorsOrigins(value: string | undefined): string[] {
  const origins: string[] = [];
  for (const entry of (value ?? '').split(',')) {
    let origin = entry.trim();
    while (origin.length > 1 && origin.endsWith('/')) {
      origin = origin.slice(0, -1);
    }
    if (origin && !origins.includes(origin)) origins.push(origin);
  }
  return origins;
}

function isWildcardOrigin(origin: CorsOptions['origin']): boolean {
  // The cors package answers `Access-Control-Allow-Origin: *` when `origin`
  // is left out or is the string '*'.
  return origin === undefined || origin === '*';
}

/**
 * The origin policy when the app did not name its origins: the
 * `CORS_ORIGINS` allow-list when set; otherwise no cross-origin access in
 * production and this machine only elsewhere.
 */
function defaultCorsOrigin(env: Record<string, string | undefined>): {
  origin: NonNullable<CorsOptions['origin']>;
  configured: boolean;
} {
  const listed = parseCorsOrigins(env[CORS_ORIGINS_ENV]);
  if (listed.includes('*')) {
    // Asking for every origin by name is the app's decision to make.
    return { origin: '*', configured: true };
  }
  if (listed.length > 0) {
    return { origin: listed, configured: true };
  }
  return {
    origin: env.NODE_ENV === 'production' ? false : LOCAL_DEVELOPMENT_ORIGIN,
    configured: false
  };
}

/**
 * `Access-Control-Allow-Origin: *` is never sent together with
 * `Access-Control-Allow-Credentials: true`. Browsers refuse that pair for
 * credentialed requests anyway, so when both would be sent the credentials
 * header is the one left out: requests without credentials behave as before
 * and nothing that worked stops working.
 */
export function withoutWildcardCredentials(options: CorsOptions): CorsOptions {
  return options.credentials === true && isWildcardOrigin(options.origin)
    ? { ...options, credentials: false }
    : options;
}

/**
 * Decide the CORS policy for an application.
 *
 * - The app passed `cors` options: they are used exactly as given. (The
 *   middleware itself still never sends a wildcard origin together with
 *   credentials; see `withoutWildcardCredentials`.)
 *
 *   NOTE: options WITHOUT an `origin` allow EVERY origin. `cors: {}`, or
 *   `cors: { methods: [...] }`, is answered with
 *   `Access-Control-Allow-Origin: *`, because that is the cors package's
 *   default and an explicit option is never overridden. The secure default
 *   below applies only when `cors` is left out entirely. A startup warning
 *   says so; name the origins (`cors: { origin: [...] }`) or remove the
 *   option to use `CORS_ORIGINS`.
 * - The app passed none: only the origins listed in `CORS_ORIGINS` (comma
 *   separated, e.g. `https://app.example.com,https://admin.example.com`) are
 *   allowed. When that is unset, no cross-origin request is allowed in
 *   production (`NODE_ENV=production`), and only localhost origins are
 *   allowed elsewhere.
 *
 * Returns the options for the cors middleware and, when there is something
 * the operator should know, a warning to log once at startup.
 */
export function resolveCorsOptions(
  appCors: CorsOptions | undefined,
  env: Record<string, string | undefined> = process.env
): { options: CorsOptions; warning?: string } {
  const howToConfigure =
    `Set ${CORS_ORIGINS_ENV} to a comma separated list of allowed origins ` +
    `(for example ${CORS_ORIGINS_ENV}=https://app.example.com), or pass ` +
    '`cors: { origin: [...] }` in the application options.';

  if (appCors !== undefined) {
    if (appCors.credentials === true && isWildcardOrigin(appCors.origin)) {
      return {
        options: appCors,
        warning:
          'CORS: `credentials: true` is configured with a wildcard origin. ' +
          'Browsers reject that combination, so Access-Control-Allow-Credentials ' +
          'is not sent. To allow credentialed requests, name the origins: pass ' +
          '`cors: { origin: [...], credentials: true }` in the application options.'
      };
    }
    if (appCors.origin === undefined) {
      return {
        options: appCors,
        warning:
          'CORS: the `cors` option was passed without an `origin`, which allows ' +
          'EVERY origin (Access-Control-Allow-Origin: *). An empty `cors: {}` does ' +
          'this too. To restrict access, pass `cors: { origin: [...] }`, or remove ' +
          `the \`cors\` option and set ${CORS_ORIGINS_ENV}. To keep it and silence ` +
          "this warning, say so explicitly with `cors: { origin: '*' }`."
      };
    }
    return { options: appCors };
  }

  const { origin, configured } = defaultCorsOrigin(env);
  if (configured) {
    return { options: { origin } };
  }
  return {
    options: { origin },
    warning:
      'CORS: no allowed origins are configured, so ' +
      (env.NODE_ENV === 'production'
        ? 'browsers on other origins cannot call this service. '
        : 'only browsers on localhost can call this service (and none will in production). ') +
      howToConfigure
  };
}

/**
 * Cors middleware handler
 *
 * @param req - Express-like request object
 * @param res - Express-like response object
 * @param next - Express-like next function
 */
export function cors<
  P extends ParamsDictionary,
  ResBodyMap extends Record<number, unknown>,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends ParsedQs,
  ReqHeaders extends Record<string, string>,
  ResHeaders extends Record<string, unknown>,
  LocalsObj extends Record<string, unknown>
>(corsOptions: CorsOptions) {
  // Built once. The wildcard-with-credentials rule is applied here so that it
  // holds for every caller, whatever options reach this middleware.
  const configuredOrigin = corsOptions.origin;
  const staticHandler = corsMiddleware(withoutWildcardCredentials(corsOptions));
  const wildcardHandler = corsMiddleware({
    ...corsOptions,
    origin: '*',
    credentials: false
  });
  return (
    req: ForklaunchBaseRequest<P, ReqBody, ReqQuery, ReqHeaders>,
    res: ForklaunchResponse<
      unknown,
      ResBodyMap,
      ForklaunchResHeaders & ResHeaders,
      LocalsObj,
      never
    >,
    next?: ForklaunchNextFunction
  ) => {
    if (req.method === 'OPTIONS') {
      res.cors = true;
    }
    if (!res.getHeader) {
      res.getHeader = (key: string) => {
        return res.getHeaders()[key] as string | string[] | undefined;
      };
    }
    const corsRequest = req as typeof req & {
      headers: Record<string, string>;
    };
    const done = next ?? (() => {});
    if (
      corsOptions.credentials === true &&
      typeof configuredOrigin === 'function'
    ) {
      // An origin callback decides per request, and may answer '*'. Resolve
      // it here, exactly as the cors package would, so that answer can be
      // served without the credentials header.
      configuredOrigin(corsRequest.headers.origin, (error, origin) => {
        if (error || !origin) {
          done(error ?? undefined);
        } else if (origin === '*') {
          wildcardHandler(corsRequest, res, done);
        } else {
          corsMiddleware({ ...corsOptions, origin })(corsRequest, res, done);
        }
      });
      return;
    }
    staticHandler(corsRequest, res, done);
  };
}
