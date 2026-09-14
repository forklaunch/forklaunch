# ForkLaunch Skills for Claude Code

Skills that teach Claude Code (and developers) how to build with ForkLaunch.

## How to explain things (applies to every skill)

Whatever a skill does, explain the result to the user **like an intelligent high
schooler** first: plain language, no jargon, no acronyms the reader has not been
given. Assume someone smart who does not know this codebase.

Then, **if it is actually needed**, add the longer technical explanation below
it — file paths, type names, tradeoffs, the reasoning a maintainer would want.

The order is not optional. Leading with the technical version makes the reader
work to find out whether they care; leading with the plain version lets them
stop reading as soon as they have what they need.

Three rules that keep this honest:

- **Say what happened, not what was run.** "Your login data is already encrypted;
  the scanner was wrong" beats "patched audit.rs:126".
- **Define a term the first time you use it**, or pick a different word. If a
  sentence only parses for someone who already knows the answer, rewrite it.
- **Do not fake simplicity by leaving things out.** If a result is uncertain,
  partial, or has a caveat, that belongs in the plain-language part — it is
  usually the part that matters most.

## Available Skills

### Building & Scaffolding
- `/getting-started` — **Start here.** The driver skill: prerequisites, planning by conversation, scaffold, score after every pass, register, deploy. Routes to every other skill.
- `/studio` — Fast app generation: greenfield, existing Next.js, backend migration.
- `/cli` — All CLI commands: init, change, delete, deploy, release, sync, sdk, openapi. **Supply ALL flags or CLI hangs.**
- `/quick-reference` — Cheat sheet: imports, patterns, templates, commands at a glance.
- `/prereqs` — Bare machine to buildable: Node, pnpm, git, OrbStack, the CLI, login. Agent-executed, confirms before installing.
- `/integrations` — GitHub (App install, repo connect, autodeploy) and provider keys via `config set`. **There is no `integrate <service>`.**
- `/managed-provisioning` — Managed apps: templates, provisioning, the claim handover, and where each surface stops.
- `/managed-apps` — Managed mode runbook: the `forklaunch managed` CLI end to end (template publish vs publish-template, the three variable kinds, instance create → claim → destroy) plus failure modes (DLQ, the custom-var decrypt bug, the OAuth relay).
- `/managed-relay` — The relay: a per-product, signed, universal callback acceptor that forwards a verified provider event to the right instance. The ingest contract, the one-command install (`init module -m relay`), and the publish check. Epic OAuth is the first wired event.
- `/deploy-mode` — The two decisions a first deploy forces: single-app vs managed template, and cluster placement (platform-shared / org-shared / dedicated) with its cost and compliance consequences. Read before any first deploy.
- `/deployment-approvals` — Approval gating: why it is opt-in, resolution order, four-eyes, the two gates.

### Backend
- `/backend-patterns` — Handlers, services, entities, schemas, routes, DI, auth, feature gating.
- `/common-tasks` — Step-by-step: add endpoints, create entities, add pages, feature gating, debugging, migrations.
- `/framework` — HTTP handler definitions, route config, validation, auth, OpenAPI, MCP, SDK gen, streaming.
- `/imports-and-structure` — Import layers, module structure, file naming. **Always import from `@{{app-name}}/core`.**
- `/websockets-and-mappers` — WebSockets, real-time, log streaming, requestMapper/responseMapper.
- `/compliance` — fp property builder, defineComplianceEntity, access levels, audit CLI, encryption, tenant isolation.
- `/score` — Generate and read an Enterprise-Readiness Report Card, gate CI on a minimum score, and know what the deterministic checks can and cannot judge.
- `/score-self-heal` — run the ForkLaunch Score during development, fix the lowest-scoring criteria from their `fix`es, re-score until the threshold passes.
- `/security` — Auth surfaces, device flow, rate limiting, security events, secrets hygiene, HMAC, branch protection.
- `/observability` — OTel wiring, metrics definitions, security events, alert rules, notifiers, telemetry retention.

### Frontend
- `/frontend-patterns` — Pages, SDK client, useApi/useMutation hooks, auth, feature gating, forms, tables.
- `/tanstack` — TanStack Start: routing, server functions, SSR, data loading.
- `/design-system` — Design philosophy router: Stripe, Linear, Robinhood, Fidelity, Clinical, Airbnb, Retool, Notion.
- `/vercel-frontend` — Deploy a frontend (Next.js, Vite, Nuxt, SvelteKit, Astro) to Vercel and wire it to deployed services: origin strategy and rewrite proxying, custom domains and DNS, env vars both ways, CORS, better-auth cookies.

### Infrastructure
- `/investigator` — Diagnose a deployed app: is it up, why did a release or deploy fail, why isn't a change showing, where are the errors. **Start here when something is wrong.**
- `/infra` — `fl infra` commands: list, status, resize, config-set, stop, delete provisioned resources. JWT-only, no CI mode.
- `/infrastructure-and-utilities` — Redis cache, S3 object store, TestContainers, utilities.
- `/platform-architecture` — Modules, DDD, deployment workflow, Pulumi, multi-tenancy, worker queues.
- `/development-guidelines` — Toolchain: runtimes (node/bun), validators (zod/typebox), databases, formatters, linters, tests, workers.

### Planning & Review
- `/plan` — 4-phase plan pipeline: CEO review, eng review, diagrams, plan doc.
- `/plan-ceo-review` — CEO/founder scope challenge and premise audit.
- `/plan-eng-review` — Engineering architecture, code quality, tests, performance review.
- `/plan-design-review` — Designer's-eye review of a plan's UI/UX, rated per dimension. (gstack)
- `/plan-devex-review` — Developer-experience review for APIs, CLIs, SDKs and docs. (gstack)

## Critical Rules

1. **Import from `@{{app-name}}/core`** for schema primitives, handlers, router, validator. NEVER from `@forklaunch/validator/*` or `@forklaunch/express`.
2. **Schemas use natural notation:** `{ name: string, age: optional(number) }`. NEVER `z.object()`.
3. **Enums use const objects:** `const X = { A: 'a' } as const`. NEVER TypeScript `enum`.
4. **Handler `name` cannot contain slashes.** Use PascalCase: `'GetRestaurant'`.
5. **Handler responses must match `responses[code]`.** Do not return starter `{ message }` JSON against a domain schema.
6. **Controller calls must match service signatures.** Pass the exact command object, not one primitive field, when a service expects `{ ... }`.
7. **Follow the local service shape.** Current Studio scaffolds often use schema-derived DTO services; do not force older mapper/entity-return examples.
8. **Always `em.flush()` after mutations.**
9. **Use `forklaunch init` for structural changes.** Don't manually create service directories.
10. **Don't edit `manifest.toml` by hand.** Use `forklaunch change` commands. Note `change service --to worker` ADDS a worker and KEEPS the HTTP surface — `type = "Worker"` means "has a worker", not "is no longer a service" (see `observability-api`). Never hand-roll a `worker.ts` to avoid it.
11. **Supply ALL CLI flags** or the CLI drops into interactive mode and hangs.
12. **TSC for backend:** `cd src/modules/<service> && ./node_modules/.bin/tsc --noEmit`
13. **Studio watch logs are informational.** `tsx watch` restarts, shutdown spam, force-kill messages, and MaxListeners warnings are not repair targets if the service starts again.

## Studio Architecture Default

In Studio planning, bias toward fat services and workers:

- Start with one custom backend service as the app control plane. Pack synchronous APIs, admin endpoints, domain logic, and shared persistence into that service first.
- If background processing is necessary, generate one worker scaffold instead of a separate service plus worker. A ForkLaunch worker scaffold creates the service/worker pair; use the service side as the control plane and the worker side for jobs. Name the worker the same product/control-plane name you would have used for the service, not a separate queue-specific name.
- Do not split by domain nouns like parser, analytics, notifications, reports, or admin during initial generation. Put future splits in prose or scaling notes, not executable scaffold commands.
- Only add more custom services when loading/importing a repo whose existing deployable boundaries make separation absolutely necessary.
- Never name the custom service or worker `core`, `monitoring`, `client-sdk`, `iam`, or `billing`; those are reserved scaffold modules. Use a product-specific name such as `workspace`, `operations`, `intake`, `reports`, `patients`, or the app domain name.
- Use a two-pass Studio decision flow: ask all user-facing questions in one grouped pass, then do one review pass that locks answers/defaults and proceeds. The first pass may ask as many questions as real requirements need; do not cap it at three or create a third decision turn. In user-facing text, call the main service the `API` or `main API`, never `core API`.
- Studio has no external coding agent handoff. Do not emit copy/paste-to-agent instructions, skill references, or "HOW TO USE THIS PLAN" text in Studio plans.
- Compliance boundary: ForkLaunch-hosted Bedrock model calls are BAA-protected through ForkLaunch; nothing else is. Direct AWS account services, direct Anthropic/OpenAI keys, SendGrid, Stripe, and other processors need the user's own compliance setup.

## Quick Start for New Features

```
1. Schema:     src/modules/<svc>/domain/schemas/<name>.schema.ts
2. Entity:     src/modules/<svc>/persistence/entities/<name>.entity.ts
3. Service:    src/modules/<svc>/domain/services/<name>.service.ts
4. Controller: src/modules/<svc>/api/controllers/<name>.controller.ts
5. Routes:     src/modules/<svc>/api/routes/<name>.routes.ts
6. Wire:       registrations.ts + bootstrapper.ts
7. Export:     api/controllers/index.ts
8. Migrate:    cd src/modules/<svc> && pnpm migrate:create && pnpm migrate:up
```

## Running

> These commands describe **this repository** (the platform), where the workspace
> root and the manifest sit together. A **scaffolded app** is laid out
> differently: the manifest is at the app root and the workspace is inside the
> modules path, so `pnpm` commands run from `<app>/<modules-path>`, not the app
> root. See `/cli` → Initialize Application.

```bash
# Prerequisites
docker compose up -d                    # Start postgres, redis, minio, etc.
pnpm install                            # Install all deps

# Backend (per-module)
cd src/modules/<service> && pnpm dev    # Start a single service in dev mode (tsx watch)

# Frontend
cd client && pnpm dev                   # Next.js dev server (default: localhost:3000)

# All services at once (from repo root)
pnpm dev                                # Starts all modules + client concurrently

# Migrations
cd src/modules/<service> && pnpm migrate:create   # Create new migration
cd src/modules/<service> && pnpm migrate:up        # Run pending migrations

# Type checking
cd src/modules/<service> && ./node_modules/.bin/tsc --noEmit   # Backend
cd client && pnpm tsc --noEmit                                  # Frontend

# Tests
cd src/modules/<service> && pnpm test   # Run module tests
```

## Module Structure

```
src/modules/<module>/
├── api/
│   ├── controllers/          # handlers.get/post/put/patch/delete
│   │   └── index.ts          # re-exports all (for SDK generation)
│   ├── routes/               # forklaunchRouter definitions
│   └── middleware/
├── domain/
│   ├── services/             # business logic (NO mappers)
│   ├── schemas/              # natural object notation
│   ├── types/
│   ├── mappers/              # requestMapper/responseMapper
│   ├── enum/                 # const-as-const enums
│   └── utils/
├── persistence/
│   ├── entities/             # MikroORM @Entity (SqlBaseEntity)
│   │   └── index.ts
│   └── seeders/
├── migrations-postgresql/
├── registrations.ts          # createConfigInjector + chain
├── bootstrapper.ts           # env loading, DI container
├── server.ts                 # forklaunchExpress, routes, listen
└── package.json              # pnpm scripts for migrate, dev, test
```
