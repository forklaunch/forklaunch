---
title: "AI Coding Tools"
description: "Pre-configured AI assistant packs for accelerated ForkLaunch development with Claude Code, Cursor, VS Code, and more."
category: "Integrations"
---

# AI Coding Tools

ForkLaunch ships pre-configured AI assistant packs that give your coding tools deep understanding of the platform architecture, CLI commands, conventions, and patterns. Download the pack for your tool and start building.

## Download Packs

| Tool | Download | What you get |
|------|----------|-------------|
| Claude Code | [claude-code-integration.zip](/downloads/claude-code-integration.zip) | `.claude/skills/` directory with 11 skill modules |
| Cursor | [cursor-integration.zip](/downloads/cursor-integration.zip) | `.cursorrules` file |
| VS Code / Windsurf | [vscode-integration.zip](/downloads/vscode-integration.zip) | `.windsurfrules` file |
| Cline | [cline-integration.zip](/downloads/cline-integration.zip) | `.clinerules` file |
| Any AI tool | [universal-integration.zip](/downloads/universal-integration.zip) | `.ai-rules.md` universal rules |
| Everything | [complete-bundle.zip](/downloads/complete-bundle.zip) | All packs combined |

Extract the zip into your project root. Rules files are auto-detected by each tool on project open.

## Setup by Tool

### Claude Code

Claude Code gets the most comprehensive integration via the `.claude/skills/` directory, which auto-loads 11 skill modules covering every aspect of ForkLaunch development.

**Setup:** Extract the pack into your project root. Skills auto-load when Claude Code launches.

**Included skills:**

- `cli/SKILL.md` -- Complete reference for all 15+ CLI commands
- `framework/SKILL.md` -- Route definitions, validation, auth patterns, OpenAPI, MCP, streaming
- `backend-patterns/SKILL.md` -- Handlers, services, entities, schemas, routes, DI, auth, HMAC
- `frontend-patterns/SKILL.md` -- Next.js pages, SDK client, hooks, feature gating, forms, tables
- `platform-architecture/SKILL.md` -- Multi-module DDD architecture, deployment workflow
- `common-tasks/SKILL.md` -- Step-by-step guides for adding endpoints, entities, pages, workers
- `imports-and-structure/SKILL.md` -- 7-layer import organization, project structure, file naming
- `websockets-and-mappers/SKILL.md` -- Real-time patterns, data transformation, mapper rules
- `infrastructure-and-utilities/SKILL.md` -- Redis cache, S3 object store, testing, utilities
- `development-guidelines/SKILL.md` -- Runtimes, validators, formatters, linters, test frameworks
- `quick-reference/SKILL.md` -- Critical rules cheat sheet for fast lookup

### Cursor

Cursor integration uses the `.cursorrules` file at the project root, which auto-loads when you open the project.

**Setup:** Extract the pack. Rules auto-apply on project open.

**Coverage:** Comprehensive -- includes imports, backend patterns, frontend patterns, WebSocket patterns, framework HTTP layer, and the quick reference cheat sheet.

### VS Code / Windsurf

Uses the `.windsurfrules` file at the project root, auto-detected by Windsurf and VS Code Copilot extensions.

**Setup:** Extract the pack. Rules auto-apply on project open.

**Coverage:** Quick reference, imports and project structure, and backend patterns.

### Cline

Uses the `.clinerules` file at the project root.

**Setup:** Extract the pack. Reference it in prompts: "Follow the rules in .clinerules"

**Coverage:** Quick reference cheat sheet with critical rules, templates, and commands.

### GitHub Copilot / Devin / Other Tools

Use the `.ai-rules.md` universal rules file. Same comprehensive coverage as `.cursorrules` in a standard markdown format that works with any AI tool.

**Setup:** Extract the pack. Reference in your prompts: "Follow the guidelines in .ai-rules.md"

## What the Rules Cover

All packs enforce ForkLaunch conventions at varying levels of detail:

### Critical Rules (All Packs)

**Import from `@{{app-name}}/core`** -- Schema primitives, `handlers`, `forklaunchRouter`, `schemaValidator`, `SqlBaseEntity`, and more all come from here. Never import from `@forklaunch/validator/*`, `@forklaunch/express`, or `@modules/core` directly.

**Natural object notation for schemas** -- Write `{ name: string, age: optional(number) }`, never `z.object()` or `Type.Object()`.

**Const-as-const enums** -- Write `const X = { A: 'a' } as const; type X = ...`, never TypeScript `enum`.

**Mappers in controllers only** -- Services return entities directly, never DTOs. Controllers handle the mapping.

**No forward slashes in handler names** -- Use `'Create Service'` or `'GetService'`, never `'service/create'`.

**CLI for manifest changes** -- Use `forklaunch change application ...`, never edit `manifest.toml` directly.

**pnpm scripts for migrations** -- Use `pnpm migrate:up`, never raw migration CLI commands.

### Backend Patterns (Claude Code, Cursor, VS Code, Universal)

- Handler/controller pattern with `handlers.get/post/put/patch/delete`
- Service pattern with `em: EntityManager` params, returning entities
- Entity pattern extending `SqlBaseEntity`
- Route pattern with `forklaunchRouter`
- DI registrations with `createConfigInjector` and chained dependencies
- JWT auth (user-facing) and HMAC auth (service-to-service) configuration
- HMAC path signing rules
- Feature gating with `billingCacheService.getCachedFeatures()`
- Mapper pattern with `requestMapper`/`responseMapper`

### Frontend Patterns (Claude Code, Cursor, Universal)

- Next.js page components with `useApi` and `useMutation` hooks
- SDK client usage (`platformApi.service.method(...)`)
- Auth context (`useAuth`, `getToken`)
- Feature gating (`useFeatureAccess`, `FeatureGate` component)
- Dialog, form, and data table patterns
- Toast notifications

### WebSocket Patterns (Claude Code, Cursor, Universal)

- Modifying `server.ts` for WebSocket support (3-step process)
- Using plain `WebSocketServer` from `ws` (not `ForklaunchWebSocketServer`)
- State service pattern for connection management
- Join/welcome handshake for multi-user apps
- Node.js 24 + tsx decorator avoidance in server.ts
- Client-side React hooks with refs for high-frequency callbacks

## Feature Comparison

| Feature | Claude Code | Cursor | VS Code | Cline | Universal |
|---------|------------|--------|---------|-------|-----------|
| Import rules | Full | Full | Full | Cheat sheet | Full |
| Backend patterns | Full | Full | Full | Cheat sheet | Full |
| Frontend patterns | Full | Full | -- | -- | Full |
| WebSocket/Mappers | Full | Full | -- | -- | Full |
| Framework HTTP | Full | Full | -- | -- | Full |
| CLI reference | Full | via quick-ref | via quick-ref | via quick-ref | via quick-ref |
| Architecture | Full | via quick-ref | via quick-ref | via quick-ref | via quick-ref |
| Auto-loads | Yes | Yes | Yes | Via reference | Manual |

