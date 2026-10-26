// Execute the actual shared frontend factory and pinned Better Auth client over
// live native HTTP. DNS forwarding and the Node cookie jar are owned; both
// client implementations, registry parsing and schema validation are real.
import assert from 'node:assert/strict';
import { createPublicKey, randomUUID, verify } from 'node:crypto';
import { readFileSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import vm from 'node:vm';

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../..'
);
const require = createRequire(import.meta.url);
const factories = new Map();
let universalModule;
export async function actualUniversalSdk() {
  if (universalModule) return universalModule;
  const dependencies = process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES;
  assert.ok(dependencies, 'Actual SDK esbuild/AJV dependencies are required');
  const { build } = await import(
    pathToFileURL(path.join(dependencies, 'node_modules/esbuild/lib/main.js'))
  );
  const result = await build({
    stdin: {
      contents: `export * from ${JSON.stringify(path.join(root, 'framework/universal-sdk/index.ts'))}; export { UniversalSdk } from ${JSON.stringify(path.join(root, 'framework/universal-sdk/src/universalSdk.ts'))}; export { coerceReferenced } from ${JSON.stringify(path.join(root, 'framework/universal-sdk/src/core/coerceReferenced.ts'))}; export { ResponseSchemas, schemaPointer } from ${JSON.stringify(path.join(root, 'framework/universal-sdk/src/core/responseSchema.ts'))};`,
      resolveDir: root,
      loader: 'ts'
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    nodePaths: [path.join(dependencies, 'node_modules')],
    alias: {
      '@forklaunch/common': path.join(root, 'framework/common/index.ts')
    }
  });
  const module = { exports: {} };
  new Function('require', 'module', 'exports', result.outputFiles[0].text)(
    require,
    module,
    module.exports
  );
  universalModule = module.exports;
  return universalModule;
}
export async function frontendFactory(sourcePath) {
  if (factories.has(sourcePath)) return factories.get(sourcePath);
  const dependencies = process.env.FORKLAUNCH_TEST_BETTER_AUTH_DEPENDENCIES;
  assert.ok(dependencies, 'Better Auth 1.7.6 client fixture is required');
  const compiler =
    process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc');
  const ts = require(
    require.resolve('typescript', {
      paths: [path.dirname(realpathSync(compiler))]
    })
  );
  const [client, plugins] = await Promise.all([
    import(
      pathToFileURL(
        path.join(
          dependencies,
          'node_modules/better-auth/dist/client/index.mjs'
        )
      )
    ),
    import(
      pathToFileURL(
        path.join(
          dependencies,
          'node_modules/better-auth/dist/client/plugins/index.mjs'
        )
      )
    )
  ]);
  const universal = await actualUniversalSdk();
  const source = ts.transpileModule(readFileSync(sourcePath, 'utf8'), {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022
    }
  }).outputText;
  const exports = {};
  vm.runInNewContext(source, {
    exports,
    URL,
    require(specifier) {
      if (specifier === '@forklaunch/universal-sdk') return universal;
      if (specifier === 'better-auth/client') return client;
      if (specifier === 'better-auth/client/plugins') return plugins;
      throw new Error(`Unexpected frontend SDK dependency: ${specifier}`);
    }
  });
  const factory = exports.clientIamBetterAuthSdkClient ?? exports.iamSdkClient;
  assert.equal(typeof factory, 'function');
  factories.set(sourcePath, factory);
  return factory;
}

export async function exerciseFrontendClient({
  target,
  host,
  authBasePath,
  label,
  authorize,
  sourcePath = path.join(root, 'blueprint/client-sdk/clientSdk.ts')
}) {
  const createClient = await frontendFactory(sourcePath);
  const originalFetch = globalThis.fetch;
  const cookies = new Map();
  const requests = [];
  const publicOrigin = new URL(host).origin;
  const boundOrigin = new URL(target);
  let surface = 'auth';
  const cookieFetch = async (input, options) => {
    const request = new Request(input, options);
    const requested = new URL(request.url);
    assert.equal(
      requested.origin,
      publicOrigin,
      'Client cannot change the configured authority'
    );
    if (surface === 'auth') {
      assert.equal(request.headers.get('origin'), publicOrigin);
      assert.equal(request.credentials, 'include');
    }
    requests.push({
      method: request.method,
      path: requested.pathname,
      surface
    });
    requested.protocol = boundOrigin.protocol;
    requested.host = boundOrigin.host;
    const forwarded = new Request(requested, request);
    if (cookies.size)
      forwarded.headers.set(
        'cookie',
        [...cookies].map(([key, value]) => `${key}=${value}`).join('; ')
      );
    const response = await originalFetch(forwarded);
    for (const line of response.headers.getSetCookie()) {
      const pair = line.split(';')[0],
        index = pair.indexOf('=');
      const name = pair.slice(0, index),
        value = pair.slice(index + 1);
      if (value) cookies.set(name, value);
      else cookies.delete(name);
    }
    return response;
  };
  const registryResponse = await originalFetch(`${target}/openapi.json`);
  assert.equal(registryResponse.status, 200);
  const registryOptions = { raw: await registryResponse.json() };
  const document = registryOptions.raw.paths
    ? registryOptions.raw
    : registryOptions.raw.latest;
  const operationFor = (path) => {
    const operation = document.paths[path]?.get?.operationId;
    assert.equal(
      typeof operation,
      'string',
      `Missing registry operation for ${path}`
    );
    return operation;
  };
  const discoveryOperation = operationFor('/discovery/auth-methods');
  let sdk;
  // Better Auth captures fetch when the client is constructed. Restore the
  // global immediately; subsequent client requests use the captured transport.
  globalThis.fetch = cookieFetch;
  try {
    sdk = await createClient({
      host,
      coreHost: publicOrigin,
      authBasePath,
      registryOptions
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
  async function coreCall(operation, request) {
    surface = 'core';
    globalThis.fetch = cookieFetch;
    try {
      return await sdk.core[operation](request);
    } finally {
      globalThis.fetch = originalFetch;
      surface = 'auth';
    }
  }
  const discovery = await coreCall(discoveryOperation);
  assert.equal(discovery.code, 200);
  assert.equal(discovery.response.email, true);
  const email = `frontend-${randomUUID()}@fixture.test`;
  const password = 'frontend-client-fixture-password';
  const signup = await sdk.betterAuth.signUp.email({
    email,
    password,
    name: 'Frontend User',
    firstName: 'Frontend',
    lastName: 'User'
  });
  assert.equal(signup.error, null, `${label}: ${JSON.stringify(signup.error)}`);
  assert.equal(signup.data.user.firstName, 'Frontend');
  assert.equal(signup.data.user.lastName, 'User');
  assert.equal(signup.data.user.password, undefined);
  const identity = signup.data.user.id;
  const session = await sdk.betterAuth.getSession();
  assert.equal(session.error, null);
  assert.equal(session.data.user.id, identity);
  assert.ok(session.data.session.token);
  {
    assert.equal(typeof authorize, 'function');
    const organization = await sdk.betterAuth.$fetch('/organization/create', {
      method: 'POST',
      body: {
        name: 'Frontend organization',
        slug: `frontend-${randomUUID()}`,
        domain: 'fixture.test'
      }
    });
    assert.equal(organization.error, null);
    const params = {
      id: identity,
      organizationId: organization.data.id,
      sessionId: session.data.session.id
    };
    for (const [suffix, expected] of [
      ['surface-roles', [{ name: 'owner' }]],
      [
        'surface-permissions',
        [{ slug: 'platform:read' }, { slug: 'platform:write' }]
      ]
    ]) {
      const route = `/user/${params.id}/organizations/${params.organizationId}/sessions/${params.sessionId}/${suffix}`;
      const operation = operationFor(
        `/user/{id}/organizations/{organizationId}/sessions/{sessionId}/${suffix}`
      );
      const result = await coreCall(operation, {
        params,
        headers: { authorization: authorize(route) }
      });
      assert.equal(result.code, 200);
      assert.deepEqual(result.response, expected);
    }
  }
  const update = await sdk.betterAuth.updateUser({
    firstName: 'Updated',
    lastName: 'Consumer'
  });
  assert.equal(update.error, null);
  const updated = await sdk.betterAuth.getSession();
  assert.equal(updated.data.user.firstName, 'Updated');
  assert.equal(updated.data.user.lastName, 'Consumer');
  const token = await sdk.betterAuth.$fetch('/token', { method: 'GET' });
  const jwks = await sdk.betterAuth.$fetch('/jwks', { method: 'GET' });
  assert.equal(token.error, null);
  assert.equal(jwks.error, null);
  const [header, payload, signature] = token.data.token.split('.');
  const key = jwks.data.keys.find(
    (key) => key.kid === JSON.parse(Buffer.from(header, 'base64url')).kid
  );
  assert.ok(
    verify(
      null,
      Buffer.from(`${header}.${payload}`),
      createPublicKey({ key, format: 'jwk' }),
      Buffer.from(signature, 'base64url')
    )
  );
  const claims = JSON.parse(Buffer.from(payload, 'base64url'));
  assert.equal(claims.sub, identity);
  assert.equal(claims.email, email);
  assert.equal(claims.iss, publicOrigin);
  assert.equal(claims.aud, publicOrigin);
  assert.equal((await sdk.betterAuth.signOut()).error, null);
  assert.equal((await sdk.betterAuth.getSession()).data, null);
  const signin = await sdk.betterAuth.signIn.email({ email, password });
  assert.equal(signin.error, null);
  assert.equal(signin.data.user.id, identity);
  assert.equal(signin.data.user.firstName, 'Updated');
  assert.equal((await sdk.betterAuth.signOut()).error, null);
  const resolvedPath =
    new URL(host).pathname.replace(/\/+$/, '') ||
    (authBasePath === undefined
      ? '/api/auth'
      : `/${authBasePath.replace(/^\/+|\/+$/g, '')}`.replace(/\/+$/, ''));
  for (const request of requests.filter(
    (request) => request.surface === 'auth'
  ))
    assert.ok(
      request.path.startsWith(`${resolvedPath}/`),
      `${label}: ${request.path}`
    );
  assert.ok(
    requests.some(
      ({ method, path }) =>
        method === 'POST' && path === `${resolvedPath}/sign-up/email`
    )
  );
  assert.ok(
    requests.some(
      ({ method, path }) =>
        method === 'GET' && path === `${resolvedPath}/get-session`
    )
  );
  assert.ok(
    requests.some(
      ({ method, path }) => method === 'GET' && path === `${resolvedPath}/jwks`
    )
  );
  console.log(
    `PASS actual frontend Better Auth client: ${label}, ${requests.length} native HTTP actions, additional fields, sessions, profile update, JWT/JWKS, signout/signin`
  );
}
