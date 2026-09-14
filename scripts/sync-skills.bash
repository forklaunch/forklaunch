#!/bin/bash
# Syncs the ForkLaunch skill pack (`fl context`) from the canonical source in
# forklaunch-platform/.claude/skills into cli/assets/forklaunch-skills.
#
# Usage:
#   scripts/sync-skills.bash            # copy allowlisted skills, update source_commit
#   scripts/sync-skills.bash --check    # no writes; report drift and exit 1 if any
#
# The source is a sibling checkout by default. Override with SKILLS_SOURCE_DIR
# if forklaunch-platform lives elsewhere.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(cd "$SCRIPT_DIR/../cli" && pwd)"
DEST="$CLI_DIR/assets/forklaunch-skills"
MANIFEST="$CLI_DIR/assets/skills.toml"
SOURCE_DIR="${SKILLS_SOURCE_DIR:-$SCRIPT_DIR/../../forklaunch-platform/.claude/skills}"

CHECK=false
case "$#" in
  0) ;;
  1)
    if [[ "$1" == "--check" ]]; then
      CHECK=true
    else
      echo "usage: $0 [--check]" >&2
      exit 2
    fi
    ;;
  *)
    echo "usage: $0 [--check]" >&2
    exit 2
    ;;
esac

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "error: source skills directory not found at $SOURCE_DIR" >&2
  echo "set SKILLS_SOURCE_DIR to the path of forklaunch-platform/.claude/skills" >&2
  exit 1
fi

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found at $MANIFEST" >&2
  exit 1
fi

INCLUDE=()
while IFS= read -r entry; do
  INCLUDE+=("$entry")
done < <(sed -n '/^include = \[/,/^\]/p' "$MANIFEST" | grep -oE '"[^"]+"' | tr -d '"')

if [[ ${#INCLUDE[@]} -eq 0 ]]; then
  echo "error: no entries parsed from the include list in $MANIFEST" >&2
  exit 1
fi

# Resolve the source commit before copying, and require a clean tree, so the
# vendored content and the recorded source_commit can never disagree.
#
# Deliberately the last commit that TOUCHED the skills, not the source repo's
# HEAD. HEAD moves on every commit to forklaunch-platform -- a billing fix, a
# bot rebuilding .ai-packs -- none of which change a single vendored byte. Using
# it made `--check` cry drift constantly, and a check that is usually wrong is
# one nobody reads.
SOURCE_SHA=$(git -C "$SOURCE_DIR" log -1 --format=%H -- . 2>/dev/null || echo "unknown")
[[ -n "$SOURCE_SHA" ]] || SOURCE_SHA="unknown"
if [[ "$SOURCE_SHA" != "unknown" ]] && [[ -n "$(git -C "$SOURCE_DIR" status --porcelain -- . 2>/dev/null)" ]]; then
  echo "error: $SOURCE_DIR has uncommitted changes — commit or stash before syncing" >&2
  exit 1
fi

WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

for entry in "${INCLUDE[@]}"; do
  src="$SOURCE_DIR/$entry"
  if [[ ! -e "$src" ]]; then
    echo "error: '$entry' is in the allowlist but missing from $SOURCE_DIR" >&2
    exit 1
  fi
  cp -R "$src" "$WORKDIR/$entry"
done

if $CHECK; then
  RECORDED_SHA=$(grep -E '^source_commit = "' "$MANIFEST" | sed -E 's/^source_commit = "(.*)"/\1/')

  # CONTENT is the check that matters, so it runs first and always runs.
  #
  # It used to run second, behind a source_commit comparison that exited 1 on a
  # mismatch -- so the moment the source repo moved ahead for any reason, the
  # comparison the pack actually depends on was never reached. A real content
  # difference hid behind that noise: `deploy logs` documentation lived only in
  # the vendored copy for days, one routine `rm -rf` away from deletion, while
  # --check reported "out of sync" for an unrelated reason the whole time.
  DIFF_OUT=$(mktemp)
  trap 'rm -f "$DIFF_OUT"; rm -rf "$WORKDIR"' EXIT

  if ! diff -rq "$WORKDIR" "$DEST" > "$DIFF_OUT" 2>&1; then
    echo "skill pack CONTENT differs from $SOURCE_DIR:" >&2
    cat "$DIFF_OUT" >&2
    echo >&2
    echo "  Run scripts/sync-skills.bash to re-vendor from the source." >&2
    echo "  If the vendored copy is the one with the newer text, move that text" >&2
    echo "  into $SOURCE_DIR first -- the sync overwrites the pack from there." >&2
    exit 1
  fi

  # Content matches. A differing source_commit now means only that the skills
  # were touched in a commit that left the allowlisted files byte-identical
  # (editing a skill the pack does not ship, say). Worth reporting, not worth
  # failing a build over.
  if [[ "$SOURCE_SHA" != "unknown" ]] && [[ "$RECORDED_SHA" != "$SOURCE_SHA" ]]; then
    echo "skill pack content is in sync with $SOURCE_DIR"
    echo "note: recorded source_commit=$RECORDED_SHA, latest skills commit is $SOURCE_SHA"
    echo "      content is identical; run scripts/sync-skills.bash to refresh the record."
    exit 0
  fi

  echo "skill pack is in sync with $SOURCE_DIR"
  exit 0
else
  rm -rf "$DEST"
  mkdir -p "$DEST"
  cp -R "$WORKDIR"/. "$DEST"/
  sed -i.bak -E "s/^source_commit = \".*\"/source_commit = \"$SOURCE_SHA\"/" "$MANIFEST"
  rm -f "$MANIFEST.bak"
  echo "synced $(find "$DEST" -type f | wc -l | tr -d ' ') files from $SOURCE_DIR@$SOURCE_SHA"
fi
