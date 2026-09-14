# Proves the relay module wires into a CUSTOMIZED / DRIFTED iam service, not only
# the pristine iam-better-auth blueprint. This is the Health-Vault-shaped case
# that the old verbatim-string anchors could not handle: the terminal dependency
# chain's last entry is no longer `RetentionService`, the environment chain's last
# entry is no longer `JWKS_PUBLIC_KEY_URL`, and the last mounted router is no
# longer `complianceRouter`.
#
# The drift below is deliberately typecheck-VALID (it reuses in-scope types and
# imports), so `pnpm build` proves the structurally-injected relay wiring compiles
# on a drifted iam.
#
# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the scripts
# share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-relay-drift" ]; then
    rm -rf output/init-relay-drift
fi

mkdir -p output/init-relay-drift
cd output/init-relay-drift

# 1. Scaffold a pristine better-auth iam.
RUST_BACKTRACE=1 $FL init application relay-drift -p relay-drift -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m iam-better-auth -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

IAM_DIR="relay-drift/src/modules/iam"

# 2. Drift the iam so NONE of the old verbatim anchors are present, while keeping
# the TypeScript valid. Done in Node so the multi-line edits are exact.
cat > _drift.cjs <<'DRIFT'
const fs = require('fs');
const path = require('path');

const iamDir = process.argv[2];

// --- registrations.ts ---
const regPath = path.join(iamDir, 'registrations.ts');
let reg = fs.readFileSync(regPath, 'utf8');

// Terminal chain: append an EXTRA service after RetentionService so the last
// entry is no longer RetentionService (defeats the old service anchor). Reuses
// the already-imported RetentionService type + in-scope Orm/OtelCollector.
const retentionBlock = `  RetentionService: {
    lifetime: Lifetime.Singleton,
    type: RetentionService,
    factory: ({ Orm, OtelCollector }) =>
      new RetentionService(Orm, OtelCollector)
  }
});`;
const retentionReplacement = `  RetentionService: {
    lifetime: Lifetime.Singleton,
    type: RetentionService,
    factory: ({ Orm, OtelCollector }) =>
      new RetentionService(Orm, OtelCollector)
  },
  ManagedInstanceService: {
    lifetime: Lifetime.Singleton,
    type: RetentionService,
    factory: ({ Orm, OtelCollector }) =>
      new RetentionService(Orm, OtelCollector)
  }
});`;
if (!reg.includes(retentionBlock)) {
  throw new Error('drift: RetentionService terminal block not found in registrations.ts');
}
reg = reg.replace(retentionBlock, retentionReplacement);

// Environment chain: append an EXTRA env var after JWKS_PUBLIC_KEY_URL so the
// last entry is no longer JWKS (defeats the old env anchor).
const jwksBlock = `  JWKS_PUBLIC_KEY_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('JWKS_PUBLIC_KEY_URL')
  }
});`;
const jwksReplacement = `  JWKS_PUBLIC_KEY_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('JWKS_PUBLIC_KEY_URL')
  },
  MANAGED_MODE: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('MANAGED_MODE') ?? undefined
  }
});`;
if (!reg.includes(jwksBlock)) {
  throw new Error('drift: JWKS_PUBLIC_KEY_URL env block not found in registrations.ts');
}
reg = reg.replace(jwksBlock, jwksReplacement);

fs.writeFileSync(regPath, reg);

// --- server.ts ---
const serverPath = path.join(iamDir, 'server.ts');
let server = fs.readFileSync(serverPath, 'utf8');

// Rename the compliance router binding + its mount so the verbatim
// `app.use(complianceRouter);` anchor is gone, while staying valid via `as`.
if (!server.includes("import { complianceRouter } from './api/routes/compliance.routes';")) {
  throw new Error('drift: compliance router import not found in server.ts');
}
server = server.replace(
  "import { complianceRouter } from './api/routes/compliance.routes';",
  "import { complianceRouter as auditRouter } from './api/routes/compliance.routes';"
);
server = server.replace('app.use(complianceRouter);', 'app.use(auditRouter);');

// Remove the mount marker comment the old wiring anchored the handoff route on.
server = server.replace('//! mounts the routes to the app\n', '');

fs.writeFileSync(serverPath, server);

console.log('drift applied');
DRIFT

node _drift.cjs "$IAM_DIR"

# Sanity: confirm the old anchors are really gone before wiring.
if grep -q 'app.use(complianceRouter);' "$IAM_DIR/server.ts"; then
    echo "drift precondition failed: complianceRouter mount still present" >&2
    exit 1
fi

# 3. Wire the relay into the drifted iam.
RUST_BACKTRACE=1 $FL init module -m relay -p relay-drift

# 4. Assert the relay is wired despite the drift.
grep -q 'RelaySessionService' "$IAM_DIR/registrations.ts" || { echo "FAIL: RelaySessionService not injected" >&2; exit 1; }
grep -q 'INSTANCE_ID' "$IAM_DIR/registrations.ts" || { echo "FAIL: INSTANCE_ID not injected" >&2; exit 1; }
grep -q 'INSTANCE_HMAC_KEY' "$IAM_DIR/registrations.ts" || { echo "FAIL: INSTANCE_HMAC_KEY not injected" >&2; exit 1; }
grep -q 'ManagedInstanceService' "$IAM_DIR/registrations.ts" || { echo "FAIL: drifted service was clobbered" >&2; exit 1; }
grep -q 'MANAGED_MODE' "$IAM_DIR/registrations.ts" || { echo "FAIL: drifted env var was clobbered" >&2; exit 1; }
grep -q 'app.use(relayRouter);' "$IAM_DIR/server.ts" || { echo "FAIL: relayRouter not mounted" >&2; exit 1; }
grep -q 'auditRouter' "$IAM_DIR/server.ts" || { echo "FAIL: drifted router mount was clobbered" >&2; exit 1; }
grep -q '/relay/handoff' "$IAM_DIR/server.ts" || { echo "FAIL: handoff route not injected" >&2; exit 1; }

echo "relay wired into drifted iam; typechecking..."

# 5. Prove the injected TypeScript typechecks on the drifted iam.
cd relay-drift/src/modules

pnpm install
pnpm build
