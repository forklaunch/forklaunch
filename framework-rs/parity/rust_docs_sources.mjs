// Execute the pinned Scalar renderer and real TS registrations, then compare the
// configuration received by Scalar with the native router's served HTML.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import vm from 'node:vm';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
import { betterAuthReference } from './fixtures/rust_better_auth_reference.mjs';
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..'
);
const dependencies = process.env.FORKLAUNCH_TEST_SCALAR_DEPENDENCIES;
assert.ok(
  dependencies,
  'Set FORKLAUNCH_TEST_SCALAR_DEPENDENCIES to the pinned Scalar fixture directory'
);
const { apiReference } = await import(
  pathToFileURL(
    path.join(
      dependencies,
      'node_modules/@scalar/express-api-reference/dist/index.js'
    )
  )
);

function scalarConfiguration(html) {
  let result;
  const capture = (selector, config) => {
    assert.equal(selector, '#app');
    assert.equal(result, undefined, 'Scalar initializes exactly once');
    result = JSON.parse(JSON.stringify(config));
  };
  for (const [, attributes, code] of html.matchAll(
    /<script\b([^>]*)>([\s\S]*?)<\/script\b[^>]*>/gi
  )) {
    if (/\bsrc=/.test(attributes)) continue;
    // The official renderer may use an ESM import; provide that function at the
    // trusted boundary rather than downloading/executing browser code in Node.
    vm.runInNewContext(code.replace(/^\s*import .* from .*$/gm, ''), {
      Scalar: { createApiReference: capture },
      createApiReference: capture
    });
  }
  assert.ok(result, 'HTML initializes Scalar');
  return result;
}
function referenceConfiguration(input) {
  let html;
  apiReference({
    pageTitle: input.title,
    sources: [{ title: 'API Reference', url: input.url }, ...input.sources]
  })(null, {
    type(type) {
      assert.equal(type, 'text/html');
      return this;
    },
    send(body) {
      html = body;
    }
  });
  return scalarConfiguration(html).sources;
}

// Own only DI registration and external service boundaries. The actual options
// factory and Better Auth OpenAPI generator execute without replacement.
const registrations = {};
const injector = {
  chain(definitions) {
    Object.assign(registrations, definitions);
    return this;
  },
  validateConfigSingletons() {
    return {};
  },
  tokens() {
    return {};
  }
};
let actual;
const Empty = class {};
actual = actualValidator(root, (specifier) => {
  if (specifier === '@forklaunch/blueprint-core')
    return { ...actual.statics, schemaValidator: actual.validator };
  if (specifier === '@forklaunch/blueprint-monitoring') return { metrics: {} };
  if (specifier === '@forklaunch/core/http')
    return { OpenTelemetryCollector: Empty };
  if (specifier === '@forklaunch/core/services')
    return {
      createConfigInjector: (_validator, definitions) =>
        injector.chain(definitions),
      getEnvVar: () => undefined,
      Lifetime: { Singleton: 'singleton', Scoped: 'scoped' },
      ComplianceDataService: Empty,
      RetentionService: Empty
    };
  if (specifier === '@forklaunch/core/persistence')
    return { wrapEmWithTenantContext() {} };
  if (specifier === '@mikro-orm/postgresql')
    return { EntityManager: Empty, MikroORM: Empty };
  if (specifier === 'better-auth') return { betterAuth() {} };
  if (specifier === './auth') return { betterAuthConfig() {} };
  if (specifier === './domain/services/surfacing.service')
    return { SurfacingService: Empty };
  if (specifier === './domain/utils/encryptionContext.util')
    return { createEncryptionAwareOrm() {} };
  if (specifier === './mikro-orm.config') return { default: {} };
});
actual.load(path.join(root, 'blueprint/iam-better-auth/registrations.ts'));
const reference = await betterAuthReference(root);
const schema = JSON.parse(
  JSON.stringify(await reference.auth.api.generateOpenAPISchema())
);
const options = await registrations.ExpressApplicationOptions.factory({
  BETTER_AUTH_BASE_PATH: '/api/auth',
  CORS_ORIGINS: ['http://localhost:3000'],
  BetterAuth: reference.auth,
  SurfacingService: {}
});
const authSources = JSON.parse(JSON.stringify(options.docs.sources));
assert.deepEqual(authSources, [{ title: 'BetterAuth', content: schema }]);
assert.equal(
  new URL(authSources[0].content.servers[0].url).pathname,
  '/api/auth'
);
assert.equal(
  new URL(`${authSources[0].content.servers[0].url}/get-session`).pathname,
  '/api/auth/get-session'
);
assert.ok(authSources[0].content.paths['/get-session']);

const child = spawn(
  'cargo',
  [
    'run',
    '--quiet',
    '--manifest-path',
    'framework-rs/Cargo.toml',
    '-p',
    'forklaunch-axum',
    '--example',
    'docs_parity'
  ],
  {
    cwd: root,
    env: { ...process.env, CARGO_INCREMENTAL: '0' },
    stdio: ['pipe', 'pipe', 'inherit']
  }
);
const pending = [];
createInterface({ input: child.stdout }).on('line', (line) =>
  pending.shift().resolve(JSON.parse(line))
);
const completion = new Promise((resolve, reject) => {
  child.once('error', reject);
  child.once('exit', (code) => {
    if (code === 0) resolve();
    else reject(new Error(`Docs bridge exited ${code}`));
    for (const item of pending.splice(0))
      item.reject(new Error('Docs bridge closed'));
  });
});
completion.catch(() => {});
const native = (input) =>
  new Promise((resolve, reject) => {
    pending.push({ resolve, reject });
    child.stdin.write(`${JSON.stringify(input)}\n`);
  });
let checks = 0;
try {
  const injection =
    '</script><script>throw new Error("executed")</script>&\u2028\u2029';
  for (const sources of [
    [],
    authSources,
    [
      {
        title: 'Remote',
        url: 'https://example.com/spec.json',
        slug: 'external',
        default: true
      }
    ],
    [
      {
        title: injection,
        content: {
          openapi: '3.1.0',
          info: { title: injection, version: '1' },
          paths: {}
        },
        default: false
      }
    ],
    [
      {
        title: 'JSON string',
        content: JSON.stringify({
          openapi: '3.1.0',
          info: { title: injection, version: '1' },
          paths: {}
        })
      }
    ],
    [
      ...authSources,
      {
        title: 'Remote',
        url: 'https://example.com/spec.json?q=%22&next=%3Cscript%3E'
      }
    ]
  ]) {
    for (const title of ['API Reference', injection]) {
      const input = { title, url: '/openapi-\'"&.json', sources };
      const output = await native(input);
      assert.equal(output['/docs'].status, 200);
      assert.equal(output['/docs'].contentType, 'text/html; charset=utf-8');
      assert.deepEqual(
        scalarConfiguration(output['/docs'].body).sources,
        referenceConfiguration(input)
      );
      assert.ok(!output['/docs'].body.includes(injection));
      assert.ok(!output['/swagger'].body.includes(injection));
      let swagger;
      const context = {
        window: {},
        SwaggerUIBundle(config) {
          swagger = config;
        }
      };
      const script = output['/swagger'].body.match(
        /<script>([\s\S]*?)<\/script\b[^>]*>/i
      )[1];
      vm.runInNewContext(script, context);
      context.window.onload();
      assert.equal(swagger.url, input.url);
      checks++;
    }
  }
  console.log(
    `PASS ${checks} native/Scalar documentation source and executable configuration comparisons; actual BetterAuth registrations preserve working base URLs`
  );
} finally {
  child.stdin.end();
  await completion;
}
