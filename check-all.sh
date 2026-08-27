#!/usr/bin/env bash
# Run every storefront gate and summarise.
#
#   ./fl-check-all.sh            both storefronts
#   ./fl-check-all.sh merch      just ForkLaunch Supply
#   ./fl-check-all.sh migrated   just the migrated Graza store
#
# Assumes the services are up (./fl-demo-up.sh). Each gate proves a different
# class of thing, and they are deliberately separate: wiring can be green while
# payment is broken, and both can be green while the page is heavy enough to
# take down the machine that opens it.
set -uo pipefail

SKILL="$HOME/.claude/skills/storefront-migrate/scripts"
DEMO="$HOME/forklaunch-merch-demo"
WHICH="${1:-all}"
pass=0; fail=0; skipped=0

run() {  # run <label> <dir> <cmd...>
  local label="$1" dir="$2"; shift 2
  printf '\n\033[1m── %s\033[0m\n' "$label"
  if ( cd "$dir" && "$@" ); then
    pass=$((pass+1))
  else
    fail=$((fail+1))
  fi
}

up() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }

# psql needs the password for whichever module the gate reads.
merch_env() { export PGPASSWORD="$(grep '^DB_PASSWORD=' "$HOME/fl-store/src/modules/ecommerce/.env.local" | cut -d= -f2-)"; }
dropin_env() { export PGPASSWORD="$(grep '^DB_PASSWORD=' "$HOME/audit-dropin/src/modules/ecommerce/.env.local" | cut -d= -f2-)"; }

if [ "$WHICH" = all ] || [ "$WHICH" = merch ]; then
  if up 4310 && up 8000; then
    merch_env
    run "ForkLaunch Supply — purchase loop, bag, orders" "$DEMO" \
      node check-store.mjs --store http://localhost:4310 \
        --db forklaunch-supply-ecommerce --admin-token "${ADMIN_TOKEN:-demo-admin-token}"
  else
    echo "  SKIP merch store (need :4310 and :8000)"; skipped=$((skipped+1))
  fi
fi

if [ "$WHICH" = all ] || [ "$WHICH" = migrated ]; then
  if up 4180 && up 8110; then
    dropin_env
    run "Migrated storefront — module wiring" "$SKILL" \
      node check-wired.mjs http://localhost:4180
    run "Migrated storefront — purchase loop" "$DEMO" \
      node check-purchase.mjs --store http://localhost:4180 --db audit-dropin-db
    # The crash guard. Runs last because it is the slowest and the least
    # likely to be the thing you are iterating on.
    run "Migrated storefront — render budget" "$DEMO" \
      node check-budget.mjs --store http://localhost:4180 --site "$SKILL/output/graza.co/site"
  else
    echo "  SKIP migrated store (need :4180 and :8110)"; skipped=$((skipped+1))
  fi
fi

printf '\n\033[1m═══ %d gate(s) passed, %d failed, %d skipped ═══\033[0m\n' "$pass" "$fail" "$skipped"
exit $(( fail > 0 ? 1 : 0 ))
