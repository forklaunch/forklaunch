#!/usr/bin/env bash
# Build and test one TypeScript workspace the way the release train does
# (.claude/skills/release-train/SKILL.md). Used by ts_release.yml before every
# Version Packages PR and before every publish. Run from the repo root:
#   bash .github/workflows/scripts/ts_release/build_and_test.sh framework|blueprint
set -euo pipefail

workspace="${1:?usage: build_and_test.sh framework|blueprint}"
cd "$(dirname "$0")/../../../.."

case "$workspace" in
  framework)
    cd framework
    pnpm build
    # Same command as build_framework.yml, which is green on main.
    pnpm test
    ;;
  blueprint)
    cd blueprint
    pnpm build
    # Per package, not from the blueprint root: the root vitest config loads a
    # setup file that is not in git. The root vitest binary is used because not
    # every package lists vitest itself (implementations/billing/stripe).
    vitest="$PWD/node_modules/.bin/vitest"
    for p in billing-stripe iam-better-auth ecommerce-stripe implementations/billing/stripe; do
      echo "::group::vitest $p"
      (cd "$p" && "$vitest" run --passWithNoTests)
      echo "::endgroup::"
    done
    ;;
  *)
    echo "unknown workspace: $workspace" >&2
    exit 2
    ;;
esac
