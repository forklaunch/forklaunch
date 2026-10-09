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
  a: { userId: "user", organizationId: "A", sessionId: "sessionA" },
  b: { userId: "user", organizationId: "B", sessionId: "sessionB" },
};
const sessions = new Map([
  [
    "sessionA",
    {
      user: "user",
      activeOrganizationId: "A",
      expiresAt: new Date(Date.now() + 60000),
    },
  ],
  [
    "sessionB",
    {
      user: "user",
      activeOrganizationId: "B",
      expiresAt: new Date(Date.now() + 60000),
    },
  ],
]);
const members = new Map([
  ["user:A", { role: "owner" }],
  ["user:B", { role: "viewer" }],
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
    return query.organizationId === "A" && query.role === "custom"
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
    await service.surfaceRole({ ...scopes.a, organizationId: "B" }),
    null,
  );
  assert.equal(
    await service.surfaceRole({ ...scopes.a, userId: "other" }),
    null,
  );
});
test("expiry and removed membership immediately refuse existing signed scope", async () => {
  const s = sessions.get("sessionA"),
    member = members.get("user:A");
  s.expiresAt = new Date(0);
  assert.equal(await service.surfaceRole(scopes.a), null);
  s.expiresAt = new Date(Date.now() + 60000);
  members.delete("user:A");
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), []);
  members.set("user:A", member);
});
test("cleared or revoked session cannot reuse an earlier organization token", async () => {
  const saved = sessions.get("sessionA");
  saved.activeOrganizationId = null;
  assert.equal(await service.surfaceRole(scopes.a), null);
  assert.equal(await service.surfaceRole(scopes.b), "viewer");
  saved.activeOrganizationId = "A";
  sessions.delete("sessionA");
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), []);
  sessions.set("sessionA", saved);
});
test("custom permissions remain in the exact verified organization", async () => {
  members.set("user:A", { role: "custom" });
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.a)), [
    "bookings:read",
  ]);
  members.set("user:B", { role: "custom" });
  assert.deepEqual(asArray(await service.surfacePermissions(scopes.b)), []);
  members.set("user:A", { role: "owner" });
  members.set("user:B", { role: "viewer" });
});
test("missing legacy claims, null organization and malformed identifiers fail closed", () => {
  for (const p of [
    null,
    undefined,
    { sub: "user" },
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
  assert.deepEqual(asArray(await roles({ sub: "user" })), []);
  members.delete("user:A");
  assert.deepEqual(asArray(await roles(payload(scopes.a))), []);
  members.set("user:A", { role: "owner" });
  assert.deepEqual(asArray(await roles(payload(scopes.a))), ["owner"]);
  assert.deepEqual(asArray(await permissions(payload(scopes.b))), [
    "platform:read",
  ]);
  members.set("user:A", { role: "viewer" });
  assert.deepEqual(asArray(await roles(payload(scopes.a))), ["viewer"]);
  members.delete("user:A");
  assert.deepEqual(asArray(await roles(payload(scopes.a))), []);
  members.set("user:A", { role: "owner" });
  for (const entry of signed) {
    assert.equal(entry.method, "GET");
    assert.match(
      entry.path,
      /^\/user\/organizations\/[AB]\/sessions\/session[AB]\/surface-(roles|permissions)$/,
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
    { ...scopes.a, userId: "other" },
    { ...scopes.a, organizationId: "B" },
    { ...scopes.a, sessionId: "sessionB" },
  ])
    assert.notEqual(sign(changed), signature);
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
