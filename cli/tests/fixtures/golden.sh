# Shell helpers for golden applications; source this file.
#   golden_app NAME DEST             copy built golden app NAME to DEST
#   golden_app --scaffold NAME DEST  copy its pristine scaffold instead
#   golden_target                    the Cargo target the Rust golden apps used
# Apps are built locally on demand when FORKLAUNCH_GOLDEN_APPS_DIR is unset.
_GOLDEN_FIXTURES="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
golden_app() {
  local flag=()
  if [ "${1:-}" = --scaffold ]; then flag=(--scaffold); shift; fi
  RUSTUP_TOOLCHAIN=stable python3 "$_GOLDEN_FIXTURES/golden.py" copy "$1" "$2" ${flag[@]+"${flag[@]}"} >/dev/null
}
golden_target() {
  python3 "$_GOLDEN_FIXTURES/golden.py" target
}
