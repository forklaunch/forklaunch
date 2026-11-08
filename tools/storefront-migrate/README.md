# storefront-migrate

Migrate a live storefront (Shopify, Squarespace) onto ForkLaunch: capture every
public page as a browsable clone, verify it against the live site feature by
feature, pull the catalog into the ecommerce module, and serve the clone wired
to the module so it takes real orders.

- `MANUAL.md` — how to install and run it, what the exit codes mean.
- `EXPLAINER.md` — the five-minute version for a colleague or a prospect.
- `WIRING.md` — from clone to a store that takes money, step by step.
- `SKILL.md` — the Claude Code skill; copy this folder to `~/.claude/skills/`.
- `references/` — the catalog import contract and the manifest schema.
- `scripts/` — the tool itself (`node scripts/bin/migrate.mjs <url> --clean`).
