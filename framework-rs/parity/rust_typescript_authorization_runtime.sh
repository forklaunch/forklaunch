#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" CARGO_INCREMENTAL=0 FORKLAUNCH_SKIP_FORMAT=1
CLI="${FORKLAUNCH_CLI:-}"
if [ -z "$CLI" ]; then
    cargo build --manifest-path "$REPO_ROOT/cli/Cargo.toml"
    CLI="$(cargo metadata --manifest-path "$REPO_ROOT/cli/Cargo.toml" --no-deps --format-version 1 | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>console.log(JSON.parse(s).target_directory+"/debug/forklaunch"))')"
fi
mkdir -p "$REPO_ROOT/output/rust-typescript-authorization"
RUN_ROOT="$(mktemp -d "$REPO_ROOT/output/rust-typescript-authorization/run.XXXXXX")"
# A generated service built against this checkout's framework; no database, so
# only the documented startup settings are required.
"$CLI" init application policy-app --path "$RUN_ROOT/app" --language rust -D Policy -A ForkLaunch -L MIT \
    --framework-path "$REPO_ROOT/framework-rs"
"$CLI" init service authorization --path "$RUN_ROOT/app" --language rust -D Authorization
mv "$RUN_ROOT/app" "$RUN_ROOT/relocated"
cd "$RUN_ROOT/relocated"
python3 - "$SCRIPT_DIR/fixtures" <<'PY'
import re
import sys
from pathlib import Path
fixtures = Path(sys.argv[1])
service = Path('src/authorization')
# Declarations compile in the private contract host (no runtime dependencies);
# handlers, verifiers and the session policy stay in the service.
(service / 'api/controllers/policy_contracts.rs').write_text((fixtures / 'rust_typescript_authorization_contracts.rs').read_text())
(service / 'policy_fixture.rs').write_text((fixtures / 'rust_typescript_authorization.rs').read_text())
host = service / '.forklaunch/contracts/lib.rs'
text = host.read_text().replace('//!', '//')
text = text.replace('pub fn registry()', 'pub fn native_registry()').replace('pub fn binding_manifest()', 'pub fn native_binding_manifest()')
host.write_text('#[path="../../api/controllers/policy_contracts.rs"]\npub mod policy_fixture;\n' + text + '''
pub fn registry()->forklaunch_core::contract::ContractRegistry{let mut registry=native_registry();for route in policy_fixture::contracts(){registry.register(route);}registry}
pub fn binding_manifest()->Result<Vec<forklaunch_core::contract::assembly::BindingDeclaration>,String>{let mut declarations=native_binding_manifest()?;for route in policy_fixture::contracts(){declarations.push(forklaunch_core::contract::assembly::BindingDeclaration::http("api/controllers/policy_contracts.rs","policy_fixture",&route.operation_name,&route)?);}Ok(declarations)}
''')
routes = service / 'api/routes/mod.rs'
routes.write_text(routes.read_text().replace('ContractRouter::new(contracts::registry())', 'ContractRouter::new(contracts::native_registry())'))
# The fixture's programmatic adapter routes are merged into the generated
# router, so startup, docs, origin security, telemetry and MCP stay generated.
cargo = service / 'Cargo.toml'
text = cargo.read_text().replace('"native-server"', '"native-server", "adapter"', 1)
framework = re.search(r'path = "([^"]+)/crates/core"', text).group(1)
text = text.replace('[dependencies]\n', f'[dependencies]\nforklaunch-validator = {{ path = "{framework}/crates/validator" }}\n', 1)
# The fixture verifies JWT, basic and HMAC credentials with forklaunch-core auth.
text = re.sub(r'(\[dependencies\.forklaunch-core\]\nfeatures = \[)', r'\1\n    "auth",', text, count=1)
cargo.write_text(text)
lib = service / 'lib.rs'
lib.write_text(lib.read_text() + 'pub mod policy_fixture;\n')
application = service / 'application.rs'
text = application.read_text()
assert '    Ok(router)' in text
application.write_text(text.replace('    Ok(router)', '''    let fixture = crate::policy_fixture::register(forklaunch_axum::ForkLaunchRouter::new(
        forklaunch_core::ServiceInfo::new("authorization", env!("CARGO_PKG_VERSION")),
    )).into_router();
    Ok(router.merge(fixture))''', 1))
PY
cargo build -p authorization
cargo test --manifest-path "$REPO_ROOT/framework-rs/Cargo.toml" -p forklaunch-core --features oidc oidc
"$CLI" openapi export --path .
"$CLI" sdk generate --path . --target all --from contract
# The generated TypeScript SDK imports its runtime types from the public
# universal-sdk package; build this checkout's package once and link it.
python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); from pathlib import Path; from universal_sdk_source import copy_universal_sdk; copy_universal_sdk(Path(sys.argv[2]), Path(sys.argv[3]))' \
    "$SCRIPT_DIR/fixtures" "$REPO_ROOT" "$RUN_ROOT/universal-sdk"
npm install --prefix "$RUN_ROOT/universal-sdk" --ignore-scripts --package-lock=false
npm --prefix "$RUN_ROOT/universal-sdk" run build
mkdir -p node_modules/@forklaunch
ln -s "$RUN_ROOT/universal-sdk" node_modules/@forklaunch/universal-sdk
"${TSC_BIN:-$REPO_ROOT/framework/node_modules/.bin/tsc}" --project src/authorization/sdk/typescript/tsconfig.json --noEmit
cargo check -p authorization-sdk
BINARY="$(cargo metadata --no-deps --format-version 1 | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>console.log(JSON.parse(s).target_directory+"/debug/authorization"))')"
node "$SCRIPT_DIR/rust_typescript_authorization_runtime.mjs" "$BINARY"
FORKLAUNCH_TEST_AUTH_TEXT=1 node "$SCRIPT_DIR/rust_typescript_authorization_runtime.mjs" "$BINARY"
node "$SCRIPT_DIR/rust_mcp_service_startup.mjs" "$BINARY"
echo 'PASS generated TypeScript session policy, JWT verification, both SDK compilers and HTTP/MCP startup'
