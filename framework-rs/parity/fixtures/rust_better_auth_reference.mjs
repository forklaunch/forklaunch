// Runs the configured Better Auth module. Only SQL/ORM storage, environment and
// telemetry boundaries are owned by the fixture; plugins and HTTP handlers are real.
import { randomUUID } from 'node:crypto';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { actualValidator } from '../rust_actual_validator_fixture.mjs';

export async function betterAuthReference(root, options = {}) {
  const dependencies = options.dependencies ?? process.env.FORKLAUNCH_TEST_BETTER_AUTH_DEPENDENCIES;
  if (!dependencies)
    throw new Error(
      'Set FORKLAUNCH_TEST_BETTER_AUTH_DEPENDENCIES to the Better Auth 1.7.6 fixture directory',
    );
  const importDependency = (file) =>
    import(pathToFileURL(path.join(dependencies, 'node_modules', file)));
  const [authModule, plugins, access, { memoryAdapter }] = await Promise.all([
    importDependency('better-auth/dist/index.mjs'),
    importDependency('better-auth/dist/plugins/index.mjs'),
    importDependency('better-auth/dist/plugins/access/index.mjs'),
    importDependency('@better-auth/memory-adapter/dist/index.mjs'),
  ]);
  const database = Object.fromEntries(
    [
      'user',
      'session',
      'account',
      'verification',
      'jwks',
      'organization',
      'member',
      'invitation',
      'team',
      'teamMember',
      'organizationRole',
    ].map((name) => [name, []]),
  );
  // SQL base properties generate UUIDs when the configured auth layer leaves
  // ID generation to the ORM. Preserve that boundary without changing config.
  const wrap = (adapter) => ({
    ...adapter,
    id: 'owned-sql-uuid-fixture',
    create: (args) =>
      adapter.create({
        ...args,
        forceAllowId: true,
        data: { ...args.data, id: args.data.id ?? randomUUID() },
      }),
    transaction: (callback) =>
      adapter.transaction((transaction) => callback(wrap(transaction))),
  });
  const realAdapter = options.orm ? await importDependency('@forklaunch/better-auth-mikro-orm-fork/lib/adapter.js') : null;
  const baseURL = options.baseURL ?? 'http://localhost:3000';
  const environment = {
    BETTER_AUTH_URL: baseURL,
    ...(options.google
      ? {
          GOOGLE_CLIENT_ID: 'fixture-google-id',
          GOOGLE_CLIENT_SECRET: 'fixture-google-secret',
        }
      : {}),
  };
  let auth;
  let actual;
  actual = actualValidator(root, (specifier, _file, load) => {
    if (specifier === '@forklaunch/core/persistence')
      // The memory adapter owns plain records. ORM fixtures supply real types.
      return options.persistence ?? { isCompliantField: () => false };
    if (specifier === '@forklaunch/better-auth-mikro-orm-fork' && realAdapter)
      return realAdapter;
    if (specifier === '@forklaunch/better-auth-mikro-orm-fork')
      return {
        mikroOrmAdapter: () => (config) =>
          wrap(memoryAdapter(database)(config)),
      };
    if (specifier === '@forklaunch/common')
      return { getEnvVar: (name) => environment[name] };
    if (specifier === '@forklaunch/blueprint-core')
      return {
        ...actual.statics,
        schemaValidator: actual.validator,
        handlers: {
          get: (_validator, _path, contract, handler) => ({
            contract,
            handler,
          }),
        },
        ...load(path.join(root, 'blueprint/core/auth/rbac.ts')),
      };
    if (specifier.endsWith('/bootstrapper'))
      return {
        tokens: { BetterAuth: 'BetterAuth' },
        ci: { resolve: () => auth },
      };
    if (specifier.includes('/persistence/entities/')) {
      const name = path.basename(specifier, '.entity');
      const capitalized = name[0].toUpperCase() + name.slice(1);
      return { [capitalized]: options.entities?.[capitalized] ?? name };
    }
    if (specifier === 'better-auth') return authModule;
    if (specifier === 'better-auth/plugins') return plugins;
    if (specifier === 'better-auth/plugins/access') return access;
    if (specifier.endsWith('betterAuthErrorLogging.util'))
      return { logBetterAuthApiError() {} };
  });
  const { load } = actual;
  const config = load(
    path.join(root, 'blueprint/iam-better-auth/auth.ts'),
  ).betterAuthConfig({
    BETTER_AUTH_BASE_PATH: options.basePath ?? '/api/auth',
    CORS_ORIGINS: [baseURL],
    orm: options.orm ?? {},
    openTelemetryCollector: {
      info() {},
      warn() {},
      error() {},
      debug() {},
      level: 'error',
    },
  });
  if (options.jwt) config.plugins = config.plugins.map(plugin => plugin.id === 'jwt'
    ? plugins.jwt({ ...plugin.options, ...options.jwt,
        jwt: { ...plugin.options.jwt, ...options.jwt.jwt } })
    : plugin);
  if (options.rateLimit) config.rateLimit = options.rateLimit;
  const rateLimitPluginCalls = [];
  if (options.rateLimitPlugins) {
    config.plugins.push(...options.rateLimitPlugins.map((rule, index) => ({
      id: `fixture-rate-limit-${index}`,
      rateLimit: [{
        window: rule.window,
        max: rule.max,
        pathMatcher: path => {
          rateLimitPluginCalls.push({ index, path });
          return path.startsWith(rule.prefix);
        },
      }],
    })));
  }
  const previousSecret = process.env.BETTER_AUTH_SECRET;
  const previousSecrets = process.env.BETTER_AUTH_SECRETS;
  delete process.env.BETTER_AUTH_SECRETS;
  process.env.BETTER_AUTH_SECRET =
    'better-auth-protocol-fixture-secret-32-characters';
  try {
    auth = authModule.betterAuth(config);
    await auth.$context;
  } finally {
    if (previousSecrets !== undefined)
      process.env.BETTER_AUTH_SECRETS = previousSecrets;
    if (previousSecret === undefined) delete process.env.BETTER_AUTH_SECRET;
    else process.env.BETTER_AUTH_SECRET = previousSecret;
  }
  const routes = Object.entries(auth.api)
    .filter(([, endpoint]) => endpoint.path)
    .map(([name, endpoint]) => ({
      name,
      path: endpoint.path,
      methods: (Array.isArray(endpoint.options.method)
        ? [...endpoint.options.method]
        : [endpoint.options.method]
      ).sort(),
    }))
    .sort((a, b) => a.path.localeCompare(b.path));
  const serverOnly = Object.entries(auth.api)
    .filter(([, endpoint]) => !endpoint.path)
    .map(([name]) => name)
    .sort();
  const matches = (row, filters, entity) =>
    Object.entries(filters).every(([field, expected]) => {
      const value = row[field === 'user' ? 'userId' : field];
      if (expected && typeof expected === 'object' && '$ne' in expected)
        return expected.$ne === null ? value != null : value !== expected.$ne;
      if (expected && typeof expected === 'object' && '$gt' in expected)
        return new Date(value) > expected.$gt;
      // These columns use PostgreSQL UUID equality. Organization/member links
      // are text columns and deliberately retain exact case-sensitive equality.
      if (entity === 'session' && ['id', 'user', 'userId'].includes(field)
          && typeof value === 'string' && typeof expected === 'string')
        return value.toLowerCase() === expected.toLowerCase();
      return value === expected;
    });
  const em = {
    async find(entity, filters) {
      return database[entity].filter((row) => matches(row, filters, entity));
    },
    async findOne(entity, filters, options) {
      const rows = [...(await this.find(entity, filters))];
      if (options?.orderBy?.createdAt === 'DESC')
        rows.sort((a, b) => new Date(b.createdAt) - new Date(a.createdAt));
      return rows[0] ?? null;
    },
  };
  const { SurfacingService } = load(
    path.join(
      root,
      'blueprint/iam-better-auth/domain/services/surfacing.service.ts',
    ),
  );
  const surfacing = new SurfacingService(options.orm ? options.orm.em.fork() : em);
  function client() {
    const cookies = new Map();
    return {
      cookies,
      async call(endpoint, body, request = {}) {
        const response = await auth.handler(
          new Request(`${(await auth.$context).baseURL.replace(/\/+$/, '')}${endpoint}`, {
            method: request.method ?? (body === undefined ? 'GET' : 'POST'),
            headers: {
              origin: new URL(baseURL).origin,
              'content-type': 'application/json',
              cookie: [...cookies]
                .map(([name, value]) => `${name}=${value}`)
                .join('; '),
              ...request.headers,
            },
            ...(body === undefined ? {} : { body: JSON.stringify(body) }),
          }),
        );
        for (const cookie of response.headers.getSetCookie()) {
          const pair = cookie.split(';')[0],
            separator = pair.indexOf('=');
          const name = pair.slice(0, separator),
            value = pair.slice(separator + 1);
          if (value) cookies.set(name, value);
          else cookies.delete(name);
        }
        const text = await response.text();
        let value = text;
        try {
          value = JSON.parse(text);
        } catch {}
        return {
          status: response.status,
          body: value,
          headers: response.headers,
        };
      },
    };
  }
  async function discovery() {
    const endpoint = load(
      path.join(
        root,
        'blueprint/iam-better-auth/api/controllers/discovery.controller.ts',
      ),
    ).getAuthMethods;
    let result;
    await endpoint.handler(
      {},
      {
        status(status) {
          result = { status };
          return this;
        },
        json(body) {
          result.body = JSON.parse(JSON.stringify(body));
        },
      },
    );
    return result;
  }
  return {
    rateLimitPluginCalls,
    auth,
    config,
    database,
    routes,
    serverOnly,
    client,
    discovery,
    surfacing,
  };
}
