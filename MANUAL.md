# Storefront Migrate — how to use it

Point it at a Shopify store. It produces a faithful, browsable, offline clone of
that store's public pages, verifies the clone against the live site, and repairs
what it can. One command. It stops when the clone has every feature the live
site has, or tells you exactly what it could not fix.

## Install it as a Claude skill (the normal way)

This is a Claude Code skill. Unpack the tarball into your skills folder and
Claude Code picks it up on its own:

```bash
mkdir -p ~/.claude/skills/storefront-migrate
tar -xzf storefront-migrate.tgz -C ~/.claude/skills/storefront-migrate
```

Then, in Claude Code, say what you want: "clone https://www.the-store.com so I
can show the client". The skill reads this manual itself, runs the setup below,
runs the one command, and reports the exit code. Everything after this heading
is what the skill does for you, and how to do it by hand if you prefer.

## Before the first run

Run everything from the unpacked folder (the one containing `scripts/`,
`MANUAL.md` and `SKILL.md`). Output lands in `scripts/output/<store-domain>/`.

Three tools have to be on the machine. Check first (`bun --version`,
`ffmpeg -version`, `node --version`); install only what is missing. The tool
checks too and refuses to start without them.

```bash
curl -fsSL https://bun.sh/install | bash        # bun — catalog pull
brew install ffmpeg                             # ffmpeg — video shrink
cd scripts && npm install && npx playwright install chromium   # browser — the gates
```

`npx playwright install chromium` prints nothing when the browser is already
there. That is success.

If it says `HARNESS-FAIL` at any point, one of these is missing. It will name
which and print the fix.

## Run it

```bash
node scripts/bin/migrate.mjs https://www.the-store.com --clean
```

That is the whole thing. A page takes **6 to 40 seconds** depending on how much
the store keeps talking to analytics after it has rendered (a quiet store: 6;
an ad-heavy one: 40). The preflight (`0/4`) prints an estimate for the store
in front of it; trust that over any rule of thumb. It prints each phase as it goes: `0/4` preflight, `1/4` crawl, `2/4`
catalog, `3/4` verify. The verify phase starts by serving the clone locally
(that is when the URL appears), then checks it against the live site, repairs,
and re-checks until it converges.

After the crawl, the verify phase loads the live site twice and the clone once
for a sample of pages (a few minutes), then repairs. If it finds dead links it
re-crawls **only the pages it named**, never the whole store; on a store with a
big menu that can still be thirty or forty pages, and the budget extends to fit
them. It re-crawls a given page at most once per run, and stops early when a
round of repairs changes nothing ("the repairs have converged"). The report may
then suggest `crawl.js --only …` for what is left: the run already did that;
do not start a second run of the same store to chase it.

**It looks stuck during the crawl.** It is not. Almost all of each page's time is
spent waiting for the page to go quiet, and stores with a lot of analytics never
do — so the wait runs to its ceiling even though the page rendered seconds ago.
It also pauses ~1s between pages on purpose so it is not hammering someone's
production site.

## What "done" looks like

It prints a report, then **keeps serving the clone** at the URL it printed
(`http://127.0.0.1:4173` unless you passed `--port`) until you press Ctrl-C.
The exit code is the line just above "still serving": `✓ complete feature
fidelity` means 0, `✗ did not reach complete feature fidelity` means 1. Pass
`--no-serve` if you want the process to exit with that code instead.

The verify phase has a budget: 5 rounds or 30 minutes, whichever comes first
(`--rounds`, `--budget-min`). When a repair needs a re-crawl of many pages it
extends the budget to fit that re-crawl (it says so, with the minutes added),
under a hard cap of 150 minutes. If it stops on budget it says so ("wall-clock
budget spent") and reports what was still missing. On a 40-page store expect
the verify phase to take 30 to 55 minutes on top of the crawl.

| Exit | Meaning | What to do |
|---|---|---|
| **0** | Every feature on the live site is present and working on the clone. | Open the URL it prints. Show the client. |
| **1** | Some features are still missing. The report names each one and its repair. | Read the list. Most are fixable; some are policy (below). |
| **2** | The tools could not run, so **nothing was proved**. | Fix what it names (usually a missing prerequisite) and re-run. Do not treat this as a fidelity result. |

Exit 2 is the one to be careful with. It is not "the clone is bad" — it is "I
could not check." A clone that got exit 2 has not been verified at all.

## Headless stores (Hydrogen, Next.js, "React storefront")

Some Shopify stores are headless: a React app in the browser draws product
grids and product pages from an API on every visit. The preflight says so
(`stack: Hydrogen` or `Next.js`, verdict AMBER, a note explaining why). What
you get: the homepage and content pages capture well; collection and product
pages come through thin or empty, because the data they render from is not
reachable offline; the catalog step usually fails (below). Showing such a
client their homepage is fine. Migrating their shop needs the merchant's
Storefront API access, which is outside this tool today.

## If the catalog step fails

Phase `2/4` can print `catalog step failed`. It means the store publishes no
readable public catalog feed: headless stores, and stores that turned the feed
off. The run continues and the clone still browses; what is missing is the
product data behind the cart mapping and the ForkLaunch import. With the
merchant's admin token, `bun scripts/catalog/cli.ts pull-admin <shop-url>
--token <token>` pulls the real catalog (with inventory and SKUs, which the
public feed never has).

## Things that will show as different, and are fine

The clone is fully offline — it never contacts any third party. That is what makes
it safe to demo anywhere. It also means some things from the live site will not
render, by design:

- Review widgets (Okendo, Judge.me, Yotpo) — their data lives on their servers
- Chat bubbles, Instagram feeds, "recently viewed" — same reason
- Cookie-consent banners (Shopify's own, Consentmo, OneTrust…) — the markup is
  captured but the script that shows it is blocked
- Promo pop-ups and spin/scratch games (Alia, Klaviyo forms, Privy, Justuno…) —
  removed on the clone: their script is blocked, so a captured pop-up could
  never be closed and would sit over the page forever
- Shopify's own hosted checkout page — the clone stops at the cart

The report lists these under "not migrated by policy" with the vendor named,
separately from the missing features. Anything it could not attribute to a
vendor stays in the missing list, which is the safe direction: it is reported
as a defect until someone confirms it is not. If a client asks, the answer
is "that widget loads from a third party; on ForkLaunch you'd wire your own."

If the store is A/B testing page templates, or changed its theme after the
crawl, the report says `same template as live` for that page and re-crawls it;
that line is the live site changing, not the clone breaking.

Live stock counts, sale timers and rotating banners will also differ from the live
site. The gate ignores those on purpose — it checks that a feature exists and
works, never that a number matches.

## Knobs you may need

| Flag | What it does |
|---|---|
| `--pages N` | page budget for the crawl (default 20). Menu destinations (up to 40) are captured on top of it, so a 20-page budget on a store with a big menu captures 40 to 60 pages. |
| `--clean` | demo mode (always use it): strips trackers, keeps every click on the clone, adds the cart overlay |
| `--no-serve` | exit with the verdict's code instead of staying up serving |
| `--port N` | serve on another port (default 4173) |
| `--rounds N`, `--budget-min N` | verify/repair budget (default 5 rounds, 30 minutes) |
| `--server URL --secret KEY` | import the catalog into a running ForkLaunch ecommerce module and wire the cart to it; without these the cart is browse-only |
| `FL_CONCURRENCY=N` | pages captured at once (default 3; use 1 on a small laptop — one browser is about 450 MB) |

Output goes to `scripts/output/<store-domain>/` (`--out DIR` to change): the
`site/` folder is the clone, `features.json` the report, `manifest.json` the
handoff file for ForkLaunch.

The verify phase samples four pages — the homepage, one collection, one
product, one content page — and checks each against the live site twice.

## Two things to know before pointing it at a store

**It downloads the merchant's content.** Every image, video and line of copy.
Showing a merchant their own store is fine. Putting the clone on a public URL, or
showing store A's clone to store B, is not.

**A crawl is real traffic on a live site.** A big store is hundreds of requests.
Run it once and keep the capture; do not re-run to fiddle. If a store blocks
you, the preflight (phase `0/4`) says `RED` with the reason before any crawl
starts — do not retry against a block.

## When the clone needs a backend

The clone's cart works offline for browsing. To take real orders it needs a
ForkLaunch app with the ecommerce module running. `WIRING.md` is the whole path,
six numbered steps with the exact commands and what each has been tested on.

## Serving a clone later

After `--no-serve`, or any time after a run, serve an existing clone without
re-crawling:

```bash
bun scripts/catalog/heroserve-fl.ts scripts/output/the-store.com/site 4173
```

That is the same server the verification used (cart, search page, product
JSON all work). `python3 scripts/serve.py 4173 scripts/output/the-store.com/site`
is a plain static fallback when bun is not around.

## If something is wrong

The terminal shows the first few of each kind of missing feature; the full
list is `features.json` in the output folder. A repair marked `!` failed; its
full output is in `repair-<name>-round<N>.log` in the same folder.

The report at the end names every missing feature. To re-run just the check
against a clone you already have serving:

```bash
node scripts/check-features.mjs --live https://www.the-store.com --clone http://localhost:4173 --json ./features.json
```

`4173` is the port the migrate command serves on unless you passed `--port`.
`--json` writes the full report next to you; without it only the terminal
output exists.

`SKILL.md` has the full reference.
