// Repository-only regression: executes actual scaffold source with bounded fakes.
// Usage: node cli/tests/iam_authorization_scope.cjs /absolute/path/esbuild/lib/main.js
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const esbuild = require(process.argv[2]);
const root = path.resolve(__dirname, "../../blueprint/iam-better-auth");
const cases = [];
const test = (name, run) => cases.push({ name, run });
class Member {}
class Session {}
class OrganizationRole {}
const sdk = { user: {} },
  signed = [];
const mocks = {
  "@forklaunch/common": { safeStringify: JSON.stringify },
  crypto: require("node:crypto"),
  "@forklaunch/blueprint-core": {
    PERMISSIONS: {
      PLATFORM_READ: "platform:read",
      PLATFORM_WRITE: "platform:write",
    },
    ROLES: {
      ADMIN: "admin",
      EDITOR: "editor",
      VIEWER: "viewer",
      SYSTEM: "system",
    },
  },
  "@forklaunch/core/http": {
    generateHmacAuthHeaders: (input) => {
      signed.push(input);
      return { signature: input.path };
    },
  },
  "@forklaunch/universal-sdk": { universalSdk: async () => sdk },
};
function load(file) {
  const filename = path.resolve(root, file);
  const output = esbuild.transformSync(fs.readFileSync(filename, "utf8"), {
    loader: "ts",
    format: "cjs",
    target: "es2022",
  }).code;
  const module = { exports: {} };
  const localRequire = (name) => {
    if (mocks[name]) return mocks[name];
    if (name.endsWith("/member.entity")) return { Member };
    if (name.endsWith("/session.entity")) return { Session };
    if (name.endsWith("/organizationRole.entity")) return { OrganizationRole };
    if (name.startsWith("."))
      return load(
        path.relative(root, path.resolve(path.dirname(filename), name + ".ts")),
      );
    throw new Error("Unexpected dependency: " + name);
  };
  vm.runInNewContext(
    output,
    {
      module,
      exports: module.exports,
      require: localRequire,
      Date,
      Buffer,
      Set,
      Map,
      encodeURIComponent,
    },
    { filename },
  );
  return module.exports;
}
const { SurfacingService } = load("domain/services/surfacing.service.ts");
const { getAuthorizationScope, authorizationScopePath } = load(
  "domain/utils/authorizationScope.util.ts",
);
const factories = load("surfacing.ts");
const scopes = {
  a: {
    userId: "11111111-1111-4111-8111-111111111111",
    organizationId: "22222222-2222-4222-8222-222222222222",
    sessionId: "44444444-4444-4444-8444-444444444444",
  },
  b: {
    userId: "11111111-1111-4111-8111-111111111111",
    organizationId: "33333333-3333-4333-8333-333333333333",
    sessionId: "55555555-5555-4555-8555-555555555555",
  },
};
const sessions = new Map([
  [
    "44444444-4444-4444-8444-444444444444",
    {
      user: "11111111-1111-4111-8111-111111111111",
      activeOrganizationId: "22222222-2222-4222-8222-222222222222",
      expiresAt: new Date(Date.now() + 60000),
    },
  ],
  [
    "55555555-5555-4555-8555-555555555555",
    {
      user: "11111111-1111-4111-8111-111111111111",
      activeOrganizationId: "33333333-3333-4333-8333-333333333333",
      expiresAt: new Date(Date.now() + 60000),
    },
  ],
]);
const members = new Map([
  [
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    { role: "owner" },
  ],
  [
    "11111111-1111-4111-8111-111111111111:33333333-3333-4333-8333-333333333333",
    { role: "viewer" },
  ],
]);
const em = {
  findOne: async (entity, query, options) => {
    assert.equal(options.refresh, true);
    if (entity === Session) {
      assert.equal(
        Object.keys(query).sort().join(","),
        "activeOrganizationId,expiresAt,id,user",
      );
      const s = sessions.get(query.id);
      return s &&
        s.user === query.user &&
        s.activeOrganizationId === query.activeOrganizationId &&
        s.expiresAt > query.expiresAt.$gt
        ? s
        : null;
    }
    assert.equal(entity, Member);
    return members.get(query.userId + ":" + query.organizationId) ?? null;
  },
  find: async (entity, query) => {
    assert.equal(entity, OrganizationRole);
    return query.organizationId === "22222222-2222-4222-8222-222222222222" &&
      query.role === "custom"
      ? [{ permission: "bookings:read" }]
      : [];
  },
};
const service = new SurfacingService(em);
const payload = (s) => ({
  sub: s.userId,
  activeOrganizationId: s.organizationId,
  sessionId: s.sessionId,
});
const asArray = (v) => [...v];
test("same user concurrent sessions keep organization role and declared permissions separate", async () => {
  assert.equal(await service.surfaceRole(scopes.a), "owner");
  assert.equal(await service.surfaceRole(scopes.b), "viewer");
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), [
    "platform:read",
    "platform:write",
  ]);
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.b)), [
    "platform:read",
  ]);
  assert.equal(
    await service.surfaceRole({
      ...scopes.a,
      organizationId: "33333333-3333-4333-8333-333333333333",
    }),
    null,
  );
  assert.equal(
    await service.surfaceRole({
      ...scopes.a,
      userId: "66666666-6666-4666-8666-666666666666",
    }),
    null,
  );
});
test("expiry and removed membership immediately refuse existing signed scope", async () => {
  const s = sessions.get("44444444-4444-4444-8444-444444444444"),
    member = members.get(
      "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    );
  s.expiresAt = new Date(0);
  assert.equal(await service.surfaceRole(scopes.a), null);
  s.expiresAt = new Date(Date.now() + 60000);
  members.delete(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
  );
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), []);
  members.set(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    member,
  );
});
test("cleared or revoked session cannot reuse an earlier organization token", async () => {
  const saved = sessions.get("44444444-4444-4444-8444-444444444444");
  saved.activeOrganizationId = null;
  assert.equal(await service.surfaceRole(scopes.a), null);
  assert.equal(await service.surfaceRole(scopes.b), "viewer");
  saved.activeOrganizationId = "22222222-2222-4222-8222-222222222222";
  sessions.delete("44444444-4444-4444-8444-444444444444");
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), []);
  sessions.set("44444444-4444-4444-8444-444444444444", saved);
});
test("custom permissions remain in the exact verified organization", async () => {
  members.set(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    { role: "custom" },
  );
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), [
    "bookings:read",
  ]);
  members.set(
    "11111111-1111-4111-8111-111111111111:33333333-3333-4333-8333-333333333333",
    { role: "custom" },
  );
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.b)), []);
  members.set(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    { role: "owner" },
  );
  members.set(
    "11111111-1111-4111-8111-111111111111:33333333-3333-4333-8333-333333333333",
    { role: "viewer" },
  );
});
test("missing legacy claims, null organization and malformed identifiers fail closed", () => {
  for (const p of [
    null,
    undefined,
    { sub: "11111111-1111-4111-8111-111111111111" },
    { ...payload(scopes.a), activeOrganizationId: null },
    { ...payload(scopes.a), sessionId: "../other" },
  ])
    assert.equal(getAuthorizationScope(p), null);
});
test("remote roles and permissions use fresh scoped server decisions and signed scope", async () => {
  const cache = new Proxy(
    {},
    {
      get() {
        throw new Error("Authorization must not use a cache");
      },
    },
  );
  sdk.user.surfaceScopedRoles = async (req) => ({
    code: 200,
    response: (await service.surfaceRole({
      userId: req.params.id,
      organizationId: req.params.organizationId,
      sessionId: req.params.sessionId,
    }))
      ? [
          {
            name: await service.surfaceRole({
              userId: req.params.id,
              organizationId: req.params.organizationId,
              sessionId: req.params.sessionId,
            }),
          },
        ]
      : [],
  });
  sdk.user.surfaceScopedPermissions = async (req) => ({
    code: 200,
    response: (
      await service.surfacePermissions({
        userId: req.params.id,
        organizationId: req.params.organizationId,
        sessionId: req.params.sessionId,
      })
    ).map((slug) => ({ slug })),
  });
  const args = {
    authCacheService: cache,
    iamUrl: "https://owned.invalid",
    hmacSecretKey: "synthetic-test-only",
  };
  const roles = await factories.createSurfaceRoles(args),
    permissions = await factories.createSurfacePermissions(args);
  assert.deepEqual(
    asArray(await roles({ sub: "11111111-1111-4111-8111-111111111111" })),
    [],
  );
  members.delete(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
  );
  assert.deepEqual(asArray(await roles(payload(scopes.a))), []);
  members.set(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    { role: "owner" },
  );
  assert.deepEqual(asArray(await roles(payload(scopes.a))), ["owner"]);
  assert.deepEqual(asArray(await permissions(payload(scopes.b))), [
    "platform:read",
  ]);
  members.set(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    { role: "viewer" },
  );
  assert.deepEqual(asArray(await roles(payload(scopes.a))), ["viewer"]);
  members.delete(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
  );
  assert.deepEqual(asArray(await roles(payload(scopes.a))), []);
  members.set(
    "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222",
    { role: "owner" },
  );
  for (const entry of signed) {
    assert.equal(entry.method, "GET");
    assert.ok(
      new Set([
        authorizationScopePath(scopes.a, "roles"),
        authorizationScopePath(scopes.a, "permissions"),
        authorizationScopePath(scopes.b, "roles"),
        authorizationScopePath(scopes.b, "permissions"),
      ]).has(entry.path),
    );
  }
  assert.notEqual(
    authorizationScopePath(scopes.a, "roles"),
    authorizationScopePath(scopes.b, "roles"),
  );
});
test("local factories require the same scope and do not fall back to user-only services", async () => {
  const local = factories.createSurfaceRolesLocally({
    authCacheService: {},
    userService: {
      surfaceRolesScoped: async (scope) => [
        { name: await service.surfaceRole(scope) },
      ],
    },
  });
  assert.deepEqual(asArray(await local(payload(scopes.b))), ["viewer"]);
  const old = factories.createSurfaceRolesLocally({
    authCacheService: {},
    userService: { surfaceRoles: async () => [{ name: "admin" }] },
  });
  assert.deepEqual(asArray(await old(payload(scopes.a))), []);
});
test("actual HMAC signature changes for every authorization scope component", () => {
  const { createHmacToken } = load(
    "../../framework/core/src/http/createHmacToken.ts",
  );
  const sign = (scope) =>
    createHmacToken({
      secretKey: "synthetic-test-only",
      method: "GET",
      path: authorizationScopePath(scope, "roles"),
      timestamp: new Date(0),
      nonce: "fixed-test-nonce",
    });
  const signature = sign(scopes.a);
  for (const changed of [
    { ...scopes.a, userId: "66666666-6666-4666-8666-666666666666" },
    { ...scopes.a, organizationId: "33333333-3333-4333-8333-333333333333" },
    { ...scopes.a, sessionId: "55555555-5555-4555-8555-555555555555" },
  ])
    assert.notEqual(sign(changed), signature);
});
test("malformed UUID scope refuses before any database call", async () => {
  let calls = 0;
  const isolated = new SurfacingService({
    findOne: async () => {
      calls++;
      throw new Error("Unexpected SQL");
    },
    find: async () => {
      calls++;
      throw new Error("Unexpected SQL");
    },
  });
  for (const field of ["userId", "organizationId", "sessionId"]) {
    for (const malformed of [
      "other-user",
      "",
      "../other",
      "11111111111141118111111111111111",
    ]) {
      const invalid = { ...scopes.a, [field]: malformed };
      assert.equal(getAuthorizationScope(payload(invalid)), null);
      assert.equal(await isolated.surfaceRole(invalid), null);
      assert.deepEqual(asArray(await isolated.surfacePermissions(invalid)), []);
    }
  }
  assert.equal(calls, 0);
});
test("valid scope preserves real database failures", async () => {
  const unavailable = new Error("Synthetic database failure");
  const isolated = new SurfacingService({
    findOne: async () => {
      throw unavailable;
    },
  });
  await assert.rejects(
    isolated.surfaceRole(scopes.a),
    (error) => error === unavailable,
  );
  await assert.rejects(
    isolated.surfacePermissions(scopes.a),
    (error) => error === unavailable,
  );
});
test("CLI BetterAuth authorization helper uses the reviewed BetterAuth source", () => {
  const template = path.resolve(
    root,
    "../../cli/src/templates/project/iam-better-auth/surfacing.ts",
  );
  assert.equal(
    fs.realpathSync(template),
    fs.realpathSync(path.join(root, "surfacing.ts")),
  );
  assert.equal(
    fs.readFileSync(template, "utf8"),
    fs.readFileSync(path.join(root, "surfacing.ts"), "utf8"),
  );
});
(async () => {
  for (const { name, run } of cases) {
    await run();
    console.log("PASS " + name);
  }
  console.log(cases.length + " authorization regressions passed");
})().catch(() => {
  console.error("Authorization regression failed");
  process.exitCode = 1;
});
