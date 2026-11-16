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
# The website's sign-in address may be a path on the site (a preview) or empty
# (a published site). The sign-in client refuses anything but a full address,
# so the starter must resolve it against the page: a relative one once left a
# published site blank.
node --input-type=module - "$work/apps/web/src/auth.ts" <<'CHECK'
import { readFileSync } from 'node:fs';
const source = readFileSync(process.argv[2], 'utf8');
const body = source.match(/export const iamAddress = [^;]+;/s);
if (!body) throw new Error('the web starter no longer resolves its sign-in address');
const iamAddress = new Function(
  `${body[0].replace('export const', 'const').replace(/: string( \| undefined)?/g, '')} return iamAddress;`
)();
const cases = [
  [undefined, 'https://app.example.com', 'https://app.example.com/api/auth'],
  ['', 'https://app.example.com', 'https://app.example.com/api/auth'],
  ['/', 'https://app.example.com', 'https://app.example.com/api/auth'],
  ['/preview/abc', 'https://desk.example.com', 'https://desk.example.com/preview/abc/api/auth'],
  ['http://localhost:8000', 'http://localhost:5173', 'http://localhost:8000/api/auth']
];
for (const [setting, page, expected] of cases) {
  const actual = iamAddress(setting, page);
  if (actual !== expected) throw new Error(`${JSON.stringify(setting)} on ${page}: ${actual}`);
}
if (!source.includes('iamAddress(import.meta.env.VITE_IAM_URL, window.location.origin)'))
  throw new Error('the sign-in client is not given the resolved address');
CHECK
printf 'my edits\n' > "$work/apps/web/src/main.tsx"
if "$CLI" init frontend --path "$work" --name 'Other' --target both; then
  echo 'Existing frontend was not protected' >&2
  exit 1
fi
test "$(cat "$work/apps/web/src/main.tsx")" = 'my edits'
echo 'CLI frontend creation, sign-in address, dry run and existing-file preservation passed.'
