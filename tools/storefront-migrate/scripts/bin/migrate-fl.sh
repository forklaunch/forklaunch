#!/usr/bin/env bash
# migrate-fl — one command: migrate a live Shopify storefront INTO the real
# ForkLaunch ecommerce module and serve it, wired to that module for a real
# browse -> cart -> checkout -> order flow.
#
#   bin/migrate-fl.sh <shop-url> [options]
#
# Options:
#   --admin-token <t>   Shopify Admin API token -> pull REAL inventory + SKUs
#                       (omit to use the public products.json feed).
#   --module <url>      Running ForkLaunch ecommerce module (default http://localhost:8001).
#   --secret <hmac>     HMAC_SECRET_KEY the module signs /catalog-import with.
#   --pages <n>         Pages to capture (default 14).
#   --port <n>          Port to serve the migrated store on (default 4700).
#   --no-serve          Import only; don't start the storefront server.
#
# Prereq: the ForkLaunch ecommerce module is running (see references/self-hosting.md
# and the boot recipe). This is the "scaffold with the CLI" entry point: point it
# at a store, and it lands in ForkLaunch.
set -euo pipefail

SHOP="${1:-}"; shift || true
[ -z "$SHOP" ] && { echo "usage: migrate-fl.sh <shop-url> [--admin-token t] [--module url] [--secret k] [--pages n] [--port n] [--no-serve]"; exit 1; }

ADMIN_TOKEN=""; MODULE="http://localhost:8001"; SECRET="${HMAC_SECRET_KEY:-}"; PAGES=14; PORT=4700; SERVE=1
while [ $# -gt 0 ]; do case "$1" in
  --admin-token) ADMIN_TOKEN="$2"; shift 2;;
  --module) MODULE="$2"; shift 2;;
  --secret) SECRET="$2"; shift 2;;
  --pages) PAGES="$2"; shift 2;;
  --port) PORT="$2"; shift 2;;
  --no-serve) SERVE=0; shift;;
  *) echo "unknown option: $1"; exit 1;;
esac; done

HERE="$(cd "$(dirname "$0")/.." && pwd)"          # scripts/
CATALOG="$HERE/catalog"
DOMAIN="$(echo "$SHOP" | sed -E 's#^https?://##; s#/.*##')"
SLUG="$(echo "$DOMAIN" | sed -E 's/[^a-zA-Z0-9]+/-/g' | tr 'A-Z' 'a-z')"
OUT="$HERE/output/$SLUG"
BUN="${BUN:-bun}"; command -v "$BUN" >/dev/null || BUN="$HOME/.bun/bin/bun"

echo "==> [1/4] Capturing $DOMAIN (native-cart, backend mode) ..."
node "$HERE/crawl.js" "$DOMAIN" "$OUT" --pages "$PAGES" --backend
SITE="$OUT/site"

echo "==> [2/4] Pulling catalog ..."
cd "$CATALOG"
if [ -n "$ADMIN_TOKEN" ]; then
  "$BUN" cli.ts pull-admin "$SHOP" --token "$ADMIN_TOKEN"     # writes data/<slug>/normalized.json (REAL inventory)
else
  "$BUN" cli.ts pull "$SHOP"
  "$BUN" cli.ts normalize "data/$SLUG/raw.json" "$SHOP"
fi
NORM="$CATALOG/data/$SLUG/normalized.json"

echo "==> [3/4] Importing catalog into the ForkLaunch module ($MODULE) ..."
[ -z "$SECRET" ] && { echo "ERROR: need --secret <HMAC_SECRET_KEY> to authenticate to the module"; exit 1; }
"$BUN" cli.ts import "$NORM" "$MODULE" "$SECRET"

if [ "$SERVE" = "1" ]; then
  echo "==> [4/4] Serving the migrated store (wired to ForkLaunch) at http://localhost:$PORT ..."
  # 5th arg is Stripe's publishable key: present -> checkout collects a card,
  # absent -> checkout just creates the order (keyless visual demo).
  exec "$BUN" "$CATALOG/heroserve-fl.ts" "$SITE" "$PORT" "$MODULE" "$SECRET" "${STRIPE_PUBLISHABLE_KEY:-}"
else
  echo "==> Done (import only). Serve later with:"
  echo "    $BUN $CATALOG/heroserve-fl.ts $SITE $PORT $MODULE <secret> [stripe-publishable-key]"
fi
