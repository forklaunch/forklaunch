#!/usr/bin/env bash
# Verify the vendored skill pack and its allowlist agree.
#
# `sync-skills.bash` does `rm -rf` on the pack and then copies exactly what
# `skills.toml` lists. So anything vendored but absent from the allowlist is not
# merely undocumented — it is scheduled for deletion by the next sync, silently,
# in a commit about something else.
#
# That is not hypothetical. Nine skills and SETUP.md were vendored while missing
# from the allowlist, including `getting-started`, `score`, `security`, and the
# SETUP.md that /prereqs and /getting-started both link to. The next routine
# sync would have dropped all of them from what customers receive.
#
# The reverse direction matters too, for a duller reason: an allowlist entry
# with nothing vendored means the last sync could not have run cleanly.
#
# Runs offline, reads only this repository. Deliberately does NOT check whether
# the pack is current with forklaunch-platform — that needs the private repo, so
# it lives in `sync-skills.bash --check` and runs where the source is available.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ASSETS_DIR="$(cd "$SCRIPT_DIR/../assets" && pwd)"
PACK_DIR="$ASSETS_DIR/forklaunch-skills"
MANIFEST="$ASSETS_DIR/skills.toml"

for path in "$PACK_DIR" "$MANIFEST"; do
  if [[ ! -e "$path" ]]; then
    echo "error: expected $path to exist" >&2
    exit 1
  fi
done

# Entries are directories (skills) or loose files (README.md, SETUP.md).
allowed="$(sed -n '/^include = \[/,/^\]/p' "$MANIFEST" |
  grep -oE '"[^"]+"' | tr -d '"' | sort -u)"

if [[ -z "$allowed" ]]; then
  echo "error: no entries parsed from the include list in $MANIFEST" >&2
  exit 1
fi

vendored="$(find "$PACK_DIR" -mindepth 1 -maxdepth 1 -exec basename {} \; | sort -u)"

fail=0

missing_from_allowlist="$(comm -13 <(echo "$allowed") <(echo "$vendored"))"
if [[ -n "$missing_from_allowlist" ]]; then
  echo "Vendored but not in the allowlist — the next sync would DELETE these:" >&2
  echo "$missing_from_allowlist" | sed 's/^/  /' >&2
  echo "  Add them to include = [...] in cli/assets/skills.toml." >&2
  fail=1
fi

missing_from_pack="$(comm -23 <(echo "$allowed") <(echo "$vendored"))"
if [[ -n "$missing_from_pack" ]]; then
  [[ $fail -eq 1 ]] && echo >&2
  echo "In the allowlist but not vendored — the last sync did not run cleanly:" >&2
  echo "$missing_from_pack" | sed 's/^/  /' >&2
  echo "  Run scripts/sync-skills.bash." >&2
  fail=1
fi

if [[ $fail -eq 0 ]]; then
  echo "Skill allowlist consistent: $(echo "$vendored" | wc -l | tr -d ' ') entries vendored, all listed."
fi

exit $fail
