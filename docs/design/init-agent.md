# `forklaunch init agent`: agents as a first-class project type

Status: accepted, in progress (2026-10-20). Applies to the TypeScript and Rust
CLIs. Both must emit the same manifest entry, routes, JSON shapes and events,
so the desktop, platform and skills work the same for either language.

## Why

Agents are written from scratch by Codex during each build today. The desktop's
generation prompt carries agent-specific rules about mounts, schemas and test
layout because the model keeps getting them wrong. Nothing reports what tools
an agent has, so the desktop's Test agent surface cannot list them. Tool
calling, approvals and credentials are reinvented per app.

## Goals

- `forklaunch init agent <name>` creates a runnable, tested agent project.
- Tools follow the Model Context Protocol (MCP) tool shape, and the agent can
  use three kinds of tool source: its own code, the app's services, and
  external MCP servers or API connectors.
- One registry is the source of truth for what the model can call and what
  `GET /api/v1/agent/tools` reports.
- Tools that change data need explicit user approval before they run.
- Credentials never live in source; they are settings (env vars).
- Codex learns how to add tools from an `agents` skill instead of the prompt.

Non-goals for v1: multi-agent orchestration, long-term memory, file uploads,
background or scheduled agent runs (use a worker), browser tools.

## Manifest

An agent is a Service with a variant tag, the same precedent as storefront and
the IAM modules:

```toml
[[projects]]
type = "Service"
name = "assistant"
variant = "agent"
description = "..."
```

Release, deploy, Service Connect, preview and sync see an ordinary HTTP
service, so the platform needs no changes. Code that cares (desktop Test agent,
skills, `forklaunch change`) checks `variant == "agent"`. An agent has no
database by default; `--database` adds one, as for services.

## Command

```
forklaunch init agent <name>
  [-p, --path <app path>]
  [-D, --description <text>]
  [--instructions <text>]        # default: a safe read-only assistant
  [--service <name>]...          # expose these app services as tools
  [--mcp <name>=<url>]...        # external MCP servers (streamable HTTP)
  [-d, --database <db>]          # optional, off by default
  [-n, --dryrun]
```

It generates the service scaffold (package, tsconfig, docker-compose entry,
pnpm workspace, manifest entry with `variant = "agent"`), then the agent files.
The agent's port follows the service allocation. `--service` and `--mcp` only
write entries into `agent.config.ts`; they can be edited later.

## Generated layout (TypeScript)

```
src/modules/<name>/
  agent.config.ts              # instructions, limits, tool sources
  tools/
    index.ts                   # local tools registry
    example.tool.ts            # read-only example (current time); delete freely
  api/controllers/agent.controller.ts
  api/routes/agent.routes.ts   # mounted at /api/v1/agent
  __test__/agent.test.ts       # routes, tool listing, approval, refusal
  server.ts, bootstrapper.ts, registrations.ts, sdk.ts, index.ts
```

Runtime logic lives in the framework (`@forklaunch/core/agent`), so the
template stays thin and agents get fixes by upgrading the framework.

## Framework API: `@forklaunch/core/agent`

```ts
defineTool({
  name: 'lookup_order',              // ^[a-z][a-z0-9_]{0,63}$
  title: 'Look up an order',
  description: 'One sentence the model reads to decide when to call it.',
  inputSchema: { order_id: string }, // app schema descriptors → JSON Schema
  annotations: { readOnlyHint: true },   // MCP hints; see Approvals
  handler: async (input, ctx) => ({ ... }) // ctx: user, authorization, signal, logger
});

serviceTools('records', { include?: string[], exclude?: string[] }); // operationIds
mcpServer({ name: 'linear', url: env('LINEAR_MCP_URL'),
            headers: { authorization: env.bearer('LINEAR_MCP_TOKEN') } });

defineAgent({ name, instructions, model?, limits?, tools, services, mcpServers });
createAgentRouter(agent, { router, schemaValidator, auth }) // the four routes below
```

Tool shape matches MCP `tools/list`: `name`, `title`, `description`,
`inputSchema` (JSON Schema), `annotations` (`readOnlyHint`, `destructiveHint`,
`idempotentHint`, `openWorldHint`).

### Tool sources

1. **Local tools**: `defineTool` in `tools/`. Use for logic owned by the agent
   and for API connectors (calendar, email, Stripe and so on): a connector is a
   file of local tools wrapping one SDK, with credentials read from settings.
2. **App services**: `serviceTools(name)` reads the service's OpenAPI
   document from `<SERVICE>_URL` (the same setting services already use to
   reach each other, e.g. `RECORDS_URL`) at `/api/<VERSION>/openapi` (the
   framework answers `{ "latest": <doc>, ... }`; a bare document is also
   accepted) and turns each operation into a tool. Calls go over the service's
   normal HTTP port with the caller's `Authorization` header, so the service's
   own auth and roles apply. Each tool's input is
   `{ params?, query?, body? }`: path parameters, query parameters and the
   JSON body. Tool names are `<service>__<operationId in snake_case>`, e.g.
   `records__get_record`. Annotations come from the operation's
   `x-mcp-annotations` if set, otherwise from the method: GET/HEAD read-only;
   DELETE destructive; PUT idempotent; anything else not read-only. Health,
   docs, OpenAPI, compliance, MCP and agent routes, and operations whose body
   is not JSON, are skipped.
   Why not the services' MCP endpoints: TypeScript services serve MCP on
   `port + 2000` and Rust services on the main port, and Service Connect only
   routes the main port, so MCP between services is not reachable in
   production. OpenAPI over the main port works the same for both languages.
3. **External MCP servers**: `mcpServer({ url, headers })` over streamable
   HTTP. Their tools are namespaced `<server>__<tool>` and treated as
   open-world. stdio servers are not supported in deployed agents.

Name collisions fail at startup. A source that is unreachable at startup is
reported as `available: false` with a reason, and its tools are not offered to
the model. Sources are refreshed every 5 minutes and on `GET /tools`.

### Approvals

A tool runs without asking only if `readOnlyHint: true`. Every other tool
(including any tool without annotations) needs approval:

- The run stops and emits `approval_required` with the tool name, arguments and
  a `continuation` token.
- The token is HMAC-signed with the service's `HMAC_SECRET_KEY`, bound to the
  user id, expires after 15 minutes and carries the transcript so far (max
  256 KiB). No server-side state, so it works across replicas.
- The client resumes with `{ continuation, decision: "approve" | "deny" }`.
  Deny tells the model the user declined.
- `destructiveHint: true` tools are always shown with a warning in the UI.

### Limits (defaults, all configurable)

8 tool steps per run, 8,000-character task, 1,024 output tokens per model
call, 30 s per tool call, 64 KiB per tool result passed to the model (longer
results are truncated with a marker), 2 MiB per streamed response.

## HTTP contract (both languages)

All routes are protected with the app's normal JWT auth and roles.

`GET /api/v1/agent/tools`

```json
{ "agent": "assistant",
  "model": "luna",
  "tools": [{
    "name": "records__get_record",
    "title": "Get record",
    "description": "...",
    "inputSchema": { "type": "object", "properties": {} },
    "annotations": { "readOnlyHint": true },
    "source": { "kind": "local" | "service" | "mcp", "name": "records" },
    "requiresApproval": false,
    "available": true,
    "unavailableReason": null
  }] }
```

`POST /api/v1/agent/tasks`: `{ "task": string }` or
`{ "continuation": string, "decision": "approve" | "deny" }` returns
`{ "answer", "model", "toolCalls": [{ "id", "name", "ok" }], "approval"? }`.
`approval` (when present) has the `approval_required` fields below and no
answer yet.

`POST /api/v1/agent/tasks/stream`: same body; `text/event-stream` with one JSON
object per `data:` line:

| `type`              | fields                                          |
| ------------------- | ----------------------------------------------- |
| `status`            | `message`                                       |
| `delta`             | `text`                                          |
| `tool_call`         | `id`, `name`, `arguments`                       |
| `tool_result`       | `id`, `ok`, `summary` (≤ 200 chars, no secrets) |
| `approval_required` | `id`, `name`, `arguments`, `annotations`, `continuation` |
| `done`              | `model`                                         |
| `error`             | `message`                                       |

The existing `{ "type": "status" | "delta" | "done" | "error" }` events stay
compatible with today's Test agent page.

Because the agent is a ForkLaunch service, the framework also exposes these
routes over MCP at `port + 2000`, so other MCP clients can run the agent.

## Model calls

OpenAI-style chat completions through the model gateway
(`/models/chat/completions` and `/stream`) with `tools`
(`{ type: "function", function: { name, description, parameters } }`),
`tool_choice: "auto"`, `max_completion_tokens`, assistant `tool_calls`, and
`role: "tool"` messages with `tool_call_id`. Streamed tool calls arrive as
`delta.tool_calls[{ index, id?, function: { name?, arguments? } }]` fragments
and are joined by `index`. After `maxSteps` tool turns, one last call is made
without tools so the model must answer. Local tools are not prefixed; service
tools are `<service>__<tool>` and MCP tools `<server>__<tool>`, all matching
`^[a-z][a-z0-9_]{0,63}$`.

## Details both runtimes match

- operationIds: `ListAgentTools` (GET /tools), `RunAgentTask` (POST /tasks),
  `StreamAgentTask` (POST /tasks/stream). Because agents are services, /tools
  and /tasks are also exposed over MCP; the SSE route need not be.
- `POST /tasks` statuses: 400 invalid task or expired/foreign continuation,
  401 not signed in, 403 role, 429 model allowance reached, 502 model call
  failed or interrupted, 503 not connected to AI or missing setting (including
  `HMAC_SECRET_KEY`). The stream route reports the same failures as `error`
  events after a 200.
- `toolCalls[].name` is always filled in.
- Local tools report `source: { kind: "local", name: <agent name> }`.
- The `GET /tools` listing is sorted by tool name and bypasses the 5-minute
  source cache; runs reuse the cache.
- Remote tools whose names still don't match `^[a-z][a-z0-9_]{0,63}$` after
  snake_casing and truncation are skipped with a warning; duplicate names fail.
- Tool results passed to the model are capped at about 64 KiB (TS counts
  UTF-16 units, Rust bytes at a char boundary), with a `[truncated]` marker.

## Settings and credentials

The model connection uses the existing settings: `AGENT_MODEL`,
`PLATFORM_GATEWAY_URL`, `INSTANCE_ID` and `FORKLAUNCH_MODEL_STREAMING` (set by
the platform and the desktop preview). Connector and MCP credentials are
service settings (`LINEAR_MCP_TOKEN` and so on). They are declared in the
manifest `compliance.secrets` so deploys check them, and the desktop surfaces
them in App settings and the chat settings card. Never in source, never in
tool results or events.

## Skill: `agents`

`cli/assets/forklaunch-skills/agents/SKILL.md`, mirrored to the platform like
the other skills. Covers: when to create an agent vs add a route; `init agent`;
writing a local tool (naming, descriptions, schemas, annotations, return
shape); exposing an app service; adding an external MCP server; writing an API
connector; credentials as settings; approvals; limits; tests to write; what
not to do (no secrets in instructions, no tools without annotations, no
write tools marked read-only, no stdio MCP).

## Desktop

- Build prompt: create agents with `forklaunch init agent` and add tools with
  the `agents` skill; drop the hand-written agent rules.
- Test agent: pick the agent by `variant = "agent"`; show a Connected tools
  panel from `GET /tools` (grouped by source, read-only vs approval badges,
  unavailable reasons); render `tool_call` / `tool_result` steps; approval card
  with Approve / Deny for `approval_required`. Apps without `/tools` show "This
  agent doesn't report its tools".
- Settings that MCP servers and connectors need appear in App settings.

## Rust

Same command, manifest entry, routes, JSON shapes, events, approval token
semantics and limits, implemented in the Rust framework and CLI templates. The
Rust MCP client uses streamable HTTP. Token format may differ between
languages since each agent verifies only its own tokens.

## Rollout

1. Framework `@forklaunch/core/agent` with tests.
2. CLI `init agent` template, wiring and tests.
3. `agents` skill, mirrored to the platform.
4. Release train: framework, blueprint, CLI.
5. Desktop: pin the CLI, prompt change, Test agent tools panel and approvals.
6. Rust implementation in parallel against this contract.

Existing hand-written agents keep working; the next planned change moves them
onto an `init agent` project.
