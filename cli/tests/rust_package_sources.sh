#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/cli/target}"
export CARGO_INCREMENTAL=0
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-0}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "$SCRIPT_DIR/rust_package_sources.sh"
fi
PACKAGE_LIST="$(mktemp)"
trap 'rm -f "$PACKAGE_LIST"' EXIT
cargo package --manifest-path "$REPO_ROOT/cli/Cargo.toml" --list --allow-dirty > "$PACKAGE_LIST"
python3 - "$REPO_ROOT" "$PACKAGE_LIST" <<'PYTHON'
from pathlib import Path
import sys

root = Path(sys.argv[1])
packaged = set(Path(sys.argv[2]).read_text().splitlines())
def source_file(path):
    # Match the embedded source catalog's documented exclusion policy.
    ignored = {"target", "node_modules", ".git", "__pycache__", "dist", "artifacts", "secrets"}
    return path.is_file() and not ignored.intersection(path.parts) and not (
        path.name == ".DS_Store" or path.suffix in {".pyc", ".pyo", ".local"}
        or path.name.startswith(".env") and not path.name.endswith(".example")
    )

expected = {
    "src/sdk/schema.rs",
    "src/sdk/schema_resources.rs",
    "src/sdk/versions.rs",
    "src/dev/contracts.rs",
    "src/dev/typescript.rs",
    "src/dev/sockets.rs",
    "src/dev/sdk.rs",
    "src/templates/rust/application/Cargo.toml.template",
    "src/templates/rust/application/typescript-sdk-tsconfig.json",
    "src/templates/rust/project/Cargo.toml.template",
    "src/templates/rust/iam/Cargo.toml.template",
    "src/templates/rust/iam/api/contracts/registry.rs",
    "src/templates/rust/iam/registrations.rs",
    "src/templates/rust/project/module/server.rs",

    "src/templates/rust/cac/Cargo.toml.template",
    "src/templates/rust/cac/api/contracts/registry.rs",
    "src/templates/rust/cac/registrations.rs",
    "src/templates/rust/cac/env.example",
    "src/templates/rust/cac/README.md",
    "src/templates/rust/billing/Cargo.toml.template",
    "src/templates/rust/billing/api/contracts/registry.rs",
    "src/templates/rust/billing/registrations.rs",
    "src/templates/rust/billing/env.example",
    "src/templates/rust/billing/README.md",
    "src/templates/rust/leased-worker/lib.rs",
    "src/templates/rust/leased-worker/smoke.rs",
    "src/templates/rust/leased-worker/signals.rs",
    "src/templates/rust/kafka-worker/main.rs",
    "src/templates/rust/kafka-worker/signals.rs",
    "src/templates/rust/kafka-worker/handler.rs",
    "src/templates/rust/kafka-worker/env.example",
    "src/templates/rust/kafka-worker/README.md",
    "src/templates/rust/postgres-worker/main.rs",
    "src/templates/rust/postgres-worker/handler.rs",
    "src/templates/rust/postgres-worker/env.example",
    "src/templates/rust/postgres-worker/README.md",
    "src/templates/rust/redis-worker/main.rs",
    "src/templates/rust/redis-worker/handler.rs",
    "src/templates/rust/redis-worker/env.example",
    "src/templates/rust/redis-worker/README.md",
    "src/templates/rust/infrastructure/redis.rs",
    "src/templates/rust/infrastructure/s3.rs",
    "src/templates/rust/capabilities/mod.rs",
    "src/templates/rust/capabilities/handler.rs",
    "src/templates/rust/capabilities/nonce_store.rs",
}
unexpected = {path for path in packaged if path.startswith(("src/templates/rust/framework/", "src/templates/rust/blueprint"))}
assert not unexpected, f"Framework or blueprint mirrors packaged: {sorted(unexpected)[:5]}"
for source in (root / "cli/src/templates/sdk").rglob("*"):
    if source_file(source):
        expected.add(str(source.relative_to(root / "cli")))
# New native controllers, private build hosts, scripts and dotfiles must be
# included without relying on a manually maintained legacy template inventory.
template_root = root / "cli/src/templates/rust"
for source in template_root.rglob("*"):
    if source_file(source):
        # Cargo omits nested crates with real manifests. Active embedded catalogs
        # rename manifests to Cargo.toml.template and are checked above by owner.
        parents = list(source.relative_to(template_root).parents)[:-1]
        if any((template_root / parent / "Cargo.toml").is_file() for parent in parents):
            continue
        expected.add(str(source.relative_to(root / "cli")))
missing = expected - packaged
assert not missing, f"Rust sources missing from CLI package: {sorted(missing)}"
print(f"PASS: {len(expected)} embedded Rust source/template files included in Cargo package")
PYTHON
