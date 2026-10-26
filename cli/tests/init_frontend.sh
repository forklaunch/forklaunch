#!/usr/bin/env bash
set -euo pipefail
CLI=${FORKLAUNCH_CLI:-./target/debug/forklaunch}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/.forklaunch"
printf 'app_name = "test"\n' > "$work/.forklaunch/manifest.toml"
"$CLI" init frontend --path "$work" --name 'Labwork Buddy' --target both --dryrun
test ! -e "$work/apps"
"$CLI" init frontend --path "$work" --name 'Labwork Buddy' --target both
test -f "$work/apps/web/src/main.tsx"
test -f "$work/apps/mobile/app/account.tsx"
printf 'my edits\n' > "$work/apps/web/src/main.tsx"
if "$CLI" init frontend --path "$work" --name 'Other' --target both; then
  echo 'Existing frontend was not protected' >&2
  exit 1
fi
test "$(cat "$work/apps/web/src/main.tsx")" = 'my edits'
echo 'CLI frontend creation, dry run and existing-file preservation passed.'
