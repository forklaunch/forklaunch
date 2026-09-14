#!/usr/bin/env bash
# Verify the shipped skill pack and its index agree.
#
# The pack is embedded into the binary with include_dir!, and `forklaunch
# context` copies it into a user's project. When README.md advertises a skill
# the pack does not contain, an agent is told to invoke something that is not
# there — and the failure surfaces to a customer as "that skill doesn't exist",
# not to us as a build error. This makes that a build error.
#
# Runs offline, reads only this directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILLS_DIR="$(cd "$SCRIPT_DIR/../assets/forklaunch-skills" && pwd)"
README="$SKILLS_DIR/README.md"

fail=0

advertised="$(grep -oE '^- `/[a-z0-9-]+`' "$README" | sed 's|^- `/||; s|`$||' | sort -u)"
shipped="$(find "$SKILLS_DIR" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort)"

missing="$(comm -23 <(echo "$advertised") <(echo "$shipped"))"
if [ -n "$missing" ]; then
  echo "README advertises skills the pack does not ship:"
  echo "$missing" | sed 's/^/  \/&/'
  fail=1
fi

unindexed="$(comm -13 <(echo "$advertised") <(echo "$shipped"))"
if [ -n "$unindexed" ]; then
  echo "Pack ships skills the README does not index (nobody will find them):"
  echo "$unindexed" | sed 's/^/  \/&/'
  fail=1
fi

# A skill directory with no SKILL.md is embedded but unusable.
while IFS= read -r skill; do
  if [ ! -f "$SKILLS_DIR/$skill/SKILL.md" ]; then
    echo "Skill directory has no SKILL.md: $skill"
    fail=1
  fi
done <<< "$shipped"

# Frontmatter `name:` must match the directory, or `/name` resolves to nothing.
while IFS= read -r skill; do
  file="$SKILLS_DIR/$skill/SKILL.md"
  [ -f "$file" ] || continue
  declared="$(grep -m1 '^name:' "$file" | sed 's/^name: *//' | tr -d '"' | tr -d "'" | xargs || true)"
  if [ -n "$declared" ] && [ "$declared" != "$skill" ]; then
    echo "Skill '$skill' declares name '$declared' — /$skill will not resolve"
    fail=1
  fi
done <<< "$shipped"

if [ "$fail" -eq 0 ]; then
  echo "Skill pack consistent: $(echo "$shipped" | wc -l | xargs) skills, index matches."
fi

exit "$fail"
