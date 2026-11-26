#!/usr/bin/env bash
# Native layout and explicit framework sources must survive infrastructure edits.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CLI="${FORKLAUNCH_CLI:-$REPO_ROOT/cli/target/debug/forklaunch}"
RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/infra-sources.XXXXXX")"
trap 'fixture_status=$?; rm -rf "$RUN_ROOT"; exit "$fixture_status"' EXIT
for source in path registry; do
    options=()
    if [ "$source" = path ]; then options=(--framework-path "$REPO_ROOT/framework-rs"); fi
    if [ "$source" = registry ]; then options=(--framework-version 0.1.0); fi
    "$CLI" init application infra-check --path "$RUN_ROOT/$source" --language rust ${options[@]+"${options[@]}"}
    "$CLI" init service storage --path "$RUN_ROOT/$source" --language rust
    cp "$RUN_ROOT/$source/src/storage/Cargo.toml" "$RUN_ROOT/$source.before.toml"
    cp "$RUN_ROOT/$source/src/storage/lib.rs" "$RUN_ROOT/$source.before.rs"
    "$CLI" infra add storage redis --path "$RUN_ROOT/$source"
    "$CLI" infra add storage s3 --path "$RUN_ROOT/$source"
    python3 - "$RUN_ROOT/$source" "$source" "$REPO_ROOT" <<'PY'
from pathlib import Path
import sys,tomllib
root=Path(sys.argv[1]).resolve(); mode=sys.argv[2]
deps=tomllib.loads((root/'src/storage/Cargo.toml').read_text())['dependencies']
for feature in ['redis','s3']:
    dep=deps['forklaunch-infrastructure-'+feature]
    assert (root/f'src/storage/infrastructure/{feature}.rs').is_file()
    assert not (root/f'src/storage/src/infrastructure/{feature}.rs').exists()
    assert 'encryption' in dep['features']
    if mode=='registry':
        assert dep['version']=='=0.1.0' and 'path' not in dep,dep
    else:
        assert dep['path']==str(Path(sys.argv[3]).resolve()/f'framework-rs/crates/{feature}'),dep
assert not (root/'vendor/framework').exists()
PY
    "$CLI" infra remove storage redis --path "$RUN_ROOT/$source"
    "$CLI" infra remove storage s3 --path "$RUN_ROOT/$source"
    cmp "$RUN_ROOT/$source.before.toml" "$RUN_ROOT/$source/src/storage/Cargo.toml"
    cmp "$RUN_ROOT/$source.before.rs" "$RUN_ROOT/$source/src/storage/lib.rs"
done
echo 'PASS native infrastructure layout and path/registry dependency source preservation'
