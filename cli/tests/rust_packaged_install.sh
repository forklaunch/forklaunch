#!/usr/bin/env bash
# Install the packaged CLI outside its checkout and exercise its native templates.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export CARGO_TARGET_DIR="${FORKLAUNCH_PACKAGE_INSTALL_TARGET:-${CARGO_TARGET_DIR:-$REPO_ROOT/cli/target}}"
export CARGO_INCREMENTAL=0
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
export FORKLAUNCH_SKIP_FORMAT=1
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-0}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "$SCRIPT_DIR/rust_packaged_install.sh"
fi
RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/forklaunch-packaged-install.XXXXXX")"
trap 'rm -rf "$RUN_ROOT"' EXIT
INSTALL_FLAGS=()
if [ "${FORKLAUNCH_PACKAGE_PROFILE:-release}" = debug ]; then INSTALL_FLAGS+=(--debug); fi
PACKAGE_SOURCE="$REPO_ROOT/cli"
# Local dirty worktrees may retain historical symlink entries that Cargo's Git
# status reader cannot traverse. CI always packages its committed checkout.
if [ "${FORKLAUNCH_PACKAGE_SOURCE_SNAPSHOT:-0}" = 1 ]; then
    if [ "${CI:-}" = true ]; then
        echo 'Source snapshot fallback must not bypass committed-checkout packaging in CI' >&2
        exit 1
    fi
    python3 - "$REPO_ROOT" "$RUN_ROOT/package-inputs" <<'PY'
from pathlib import Path
import shutil,sys,tomllib
repo,destination=map(Path,sys.argv[1:])
cli=repo/'cli'
manifest=tomllib.loads((cli/'Cargo.toml').read_text())
def ignore(directory,names):
    skipped={name for name in names if name in {'target','node_modules','.git','__pycache__'}}
    if Path(directory)==cli/'tests': skipped.add('output')
    return skipped
def copy(source,target):
    if source.is_dir(): shutil.copytree(source,target,ignore=ignore,dirs_exist_ok=True)
    else:
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(source,target)
roots={pattern.split('/')[0] for pattern in manifest['package']['include'] if not pattern.startswith('!')}
for name in sorted(roots):
    for source in cli.glob(name): copy(source,destination/'cli'/source.name)
for name in ['Cargo.toml','Cargo.lock','crates']:
    copy(repo/'framework-rs'/name,destination/'framework-rs'/name)
print('Using explicit local source snapshot; every archived CLI source and embedded framework file remains hash-checked')
PY
    PACKAGE_SOURCE="$RUN_ROOT/package-inputs/cli"
fi
cargo package --manifest-path "$PACKAGE_SOURCE/Cargo.toml" --allow-dirty --no-verify
python3 - "$CARGO_TARGET_DIR" "$RUN_ROOT" "$REPO_ROOT" <<'PY'
from pathlib import Path
import hashlib,sys,tarfile,tomllib
repo=Path(sys.argv[3])
metadata=tomllib.loads((repo/'cli/Cargo.toml').read_text())['package']
archive=Path(sys.argv[1])/'package'/f"{metadata['name']}-{metadata['version']}.crate"
with tarfile.open(archive) as package:
    package.extractall(Path(sys.argv[2])/'source',filter='data')
source=next((Path(sys.argv[2])/'source').iterdir())
for packaged in source.rglob('*'):
    if not packaged.is_file(): continue
    relative=packaged.relative_to(source)
    if str(relative) in {'Cargo.toml','Cargo.lock','.cargo_vcs_info.json'}: continue
    original=repo/'cli'/('Cargo.toml' if str(relative)=='Cargo.toml.orig' else relative)
    assert original.is_file(), original
    assert hashlib.sha256(packaged.read_bytes()).digest()==hashlib.sha256(original.read_bytes()).digest(), packaged
for relative in ['src/dev/contracts.rs','src/dev/typescript.rs','src/dev/sockets.rs','src/dev/sdk.rs','src/sdk/workspace.rs','src/init/rust_project/native_layout.rs','src/init/rust_project/contracts_layout.rs','src/templates/rust/project/native-service/application.rs','src/templates/rust/project/native-service/bootstrapper.rs','src/templates/rust/project/service/src/error/mod.rs','src/templates/rust/capabilities/controller.rs']:
    assert (source/relative).is_file(), relative
assert not (source/'src/templates/rust/module-http').exists(), 'Module handlers come from published blueprint crates'
assert not (source/'src/templates/rust/framework').exists() and not (source/'src/templates/rust/blueprint-workspace').exists()
print('PASS packaged CLI sources match the checkout and embed no blueprint HTTP layers')
PY
SOURCE_ROOT="$(find "$RUN_ROOT/source" -mindepth 1 -maxdepth 1 -type d)"
cargo package --manifest-path "$SOURCE_ROOT/Cargo.toml" --list --allow-dirty > "$RUN_ROOT/repackaged-files.txt"
grep -qxE 'src/templates/application/\.gitignore' "$RUN_ROOT/repackaged-files.txt"
grep -qxE 'src/templates/application/\.prettierrc' "$RUN_ROOT/repackaged-files.txt"
if grep -qE '(^|/)(node_modules|target|output)/' "$RUN_ROOT/repackaged-files.txt"; then
    echo 'Generated output or dependencies leaked into portable CLI sources' >&2
    exit 1
fi
cargo generate-lockfile --manifest-path "$SOURCE_ROOT/Cargo.toml"
cargo install --path "$SOURCE_ROOT" --locked "${INSTALL_FLAGS[@]}" --root "$RUN_ROOT/installed" --target-dir "$CARGO_TARGET_DIR"
export FORKLAUNCH_CLI="$RUN_ROOT/installed/bin/forklaunch"
"$FORKLAUNCH_CLI" dev --help > "$RUN_ROOT/dev-help.txt"
grep -q -- '--compose-file' "$RUN_ROOT/dev-help.txt"
"$FORKLAUNCH_CLI" init application archive-app --path "$RUN_ROOT/app" --database postgresql --validator zod --formatter prettier --linter oxlint --http-framework express --runtime node --test-framework vitest -D 'Packaged CLI' -A ForkLaunch -L MIT
"$FORKLAUNCH_CLI" init service probe --path "$RUN_ROOT/app" --language rust --mappers --infrastructure redis s3 -D Probe
"$FORKLAUNCH_CLI" init worker jobs --path "$RUN_ROOT/app" --language rust --type database --database postgresql --runtime apalis -D Jobs
"$FORKLAUNCH_CLI" init library shared --path "$RUN_ROOT/app" --language rust -D Shared
"$FORKLAUNCH_CLI" init router notes --path "$RUN_ROOT/app/src/probe" --language rust
mv "$RUN_ROOT/app" "$RUN_ROOT/relocated-app"
cd "$RUN_ROOT/relocated-app"
export CARGO_TARGET_DIR="${FORKLAUNCH_PACKAGE_GENERATED_TARGET:-$CARGO_TARGET_DIR}"
cargo check --workspace --all-targets
cargo test -p probe --test http
cargo build -p probe -p jobs --bins
"$FORKLAUNCH_CLI" sdk generate --target all --from contract --check --path .
node "$SCRIPT_DIR/fixtures/rust_mcp_service_startup.mjs" "$CARGO_TARGET_DIR/debug/probe"
node "$SCRIPT_DIR/fixtures/rust_worker_service_policy.mjs" "$CARGO_TARGET_DIR/debug/jobs"
echo 'PASS packaged CLI installation outside checkout, portable scaffolding, service-local SDKs, infrastructure adapters and HTTP/MCP startup'
