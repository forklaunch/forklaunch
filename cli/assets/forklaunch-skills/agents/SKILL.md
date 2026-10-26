---
name: agents
description: "Build AI agents in a ForkLaunch app: `forklaunch init agent`, adding tools the MCP way (the agent's own tools, the app's services, external MCP servers, API connectors), approvals for tools that change data, credentials as settings, limits and tests. Use whenever a feature needs an assistant, chat agent, copilot or tool-calling model."
user-invokable: true
---

# Agents

## When to Use This Skill

- "Add an assistant / agent / copilot / chat" to an app
- "Let the agent look up / create / change X"
- "Connect the agent to Linear / GitHub / Google Calendar / Stripe / an MCP server"
- Changing what an existing agent can do, or why a tool is missing or unavailable

Do not hand-write agent routes or model loops. Every agent is an `init agent`
project, so the desktop Test agent page, deploys and the platform understand it.

## Create the agent

```bash
forklaunch init agent assistant \
  --description "Helps staff with orders" \
  --instructions "You help staff look up and manage orders. ..." \
  --service orders            # repeatable: expose this service's routes as tools
  --mcp linear=https://mcp.linear.app/mcp   # repeatable: external MCP server
```

Supply every flag; the command does not prompt when flags are given. It needs
IAM in the app (agents act as the signed-in user). It creates a Service with
`variant = "agent"` in the manifest and:

```
src/modules/assistant/
  agent.config.ts        # instructions, limits, tool sources: edit this
  tools/index.ts         # defineTool, bound to the app's schema validator
  tools/currentTime.tool.ts   # example read-only tool; replace it
  api/routes/agent.routes.ts  # GET /api/v1/agent/tools, POST /tasks, /tasks/stream
  __test__/agent.test.ts
```

Settings it needs: `AGENT_MODEL` and the model gateway settings (the platform
and the desktop preview set these), `HMAC_SECRET_KEY` (written by init; seals
approvals), `<SERVICE>_URL` per exposed service, `<NAME>_MCP_URL` per MCP
server, plus any connector credentials you add.

## The four kinds of tool

Pick the first that fits.

1. **An app service's routes**: the data already has an API in this app.
   Add `serviceTools('orders')` to `sources` in `agent.config.ts` (and
   `ORDERS_URL`, which `--service` writes for you). Each route becomes a tool
   named `orders__<operation>`; calls carry the user's own token, so the
   service's roles still apply. GET routes are read-only; other methods need
   approval. Narrow it: `serviceTools('orders', { include: ['GetOrder', 'ListOrders'] })`.
   Give routes clear `name` and `summary`: they become the tool name and the
   description the model reads.
2. **An external MCP server**: the provider publishes one (Linear, GitHub,
   Notion, Sentry, Stripe and others).
   ```ts
   mcpServer({
     name: 'linear',
     url: process.env.LINEAR_MCP_URL,
     headers: { authorization: process.env.LINEAR_MCP_TOKEN ? `Bearer ${process.env.LINEAR_MCP_TOKEN}` : undefined }
   })
   ```
   Streamable HTTP only; never stdio servers in a deployed agent. Tools are
   named `linear__<tool>` and keep the server's annotations.
3. **A connector**: an external API without an MCP server. Write one file per
   provider in `tools/`, e.g. `tools/googleCalendar.tool.ts`, exporting tools
   that wrap its SDK or REST API, and add them to `localTools(...)`.
4. **Agent-only logic**: calculations, formatting, lookups in data the agent
   owns. A local tool.

## Writing a tool

```ts
import { optional, string } from '@<app>/core';
import { defineTool } from './index';

export const listEvents = defineTool({
  name: 'list_calendar_events',        // lowercase snake_case, ≤ 64 chars, unique
  title: 'List calendar events',
  description: 'Events on the user’s calendar between two ISO dates. Use for "what is on my calendar".',
  inputSchema: { from: string, to: string, calendarId: optional(string) },
  annotations: { readOnlyHint: true, openWorldHint: true },
  handler: async ({ from, to, calendarId }, context) => {
    const token = process.env.GOOGLE_CALENDAR_TOKEN;       // a setting, never source
    if (!token) throw new Error('Set GOOGLE_CALENDAR_TOKEN in App settings.');
    const res = await fetch(`https://www.googleapis.com/calendar/v3/calendars/${encodeURIComponent(calendarId ?? 'primary')}/events?timeMin=${from}&timeMax=${to}`, {
      headers: { authorization: `Bearer ${token}` },
      signal: context.signal
    });
    if (!res.ok) throw new Error(`Calendar answered ${res.status}.`);
    const body = await res.json();
    return body.items.map((e: any) => ({ id: e.id, title: e.summary, start: e.start, end: e.end }));
  }
});
```

Rules:
- **Description**: one or two sentences on what it does and when to use it.
  The model only sees the name, description and schema.
- **Schema**: the same descriptors as routes. Required fields first; no free-form
  "data" blobs when you know the fields.
- **Annotations are the safety contract.** `readOnlyHint: true` only if the tool
  cannot change anything anywhere. Anything that writes, sends, books, pays,
  deletes or calls out with side effects is not read-only and so needs the
  user's approval. Add `destructiveHint: true` for deletes and irreversible
  actions, `idempotentHint: true` when repeating is harmless,
  `openWorldHint: true` for external systems.
- **Return** small JSON with just what the model needs. Results over 64 KiB are
  truncated. Never return tokens, keys or whole records the user can't see.
- **Fail** by throwing an Error with a plain message; the model reads it and
  tells the user. Honour `context.signal`. Calls time out after 30 s.
- **Act as the user**: forward `context.authorization` to this app's own
  services; never use a system or admin token to bypass the user's roles.

## Approvals

Tools without `readOnlyHint: true` pause the run. The stream sends
`approval_required` with the tool, arguments and a sealed `continuation`; the
client resumes with `{ continuation, decision: "approve" | "deny" }` on the same
route. Continuations expire after 15 minutes and only work for the same user.
The desktop Test agent shows Approve / Deny. Do not try to skip approval by
marking a write tool read-only.

## Credentials

Every key, token or URL is a setting (env var) read with `process.env` inside
the handler or `agent.config.ts`. Declare required ones in the planner's
`connections` so the desktop asks the user for them; list secrets in the
manifest `compliance.secrets` so deploys check them. Never in instructions,
source, tests, tool results or logs.

## Instructions

Write them in `agent.config.ts`: who the agent serves, what it may do, what it
must refuse, and tone. Do not list tools there (the model gets them), do not
paste data or secrets, and do not promise abilities the tools don't provide.

## Limits

Defaults: 8 tool steps per run, 8,000-character task, 1,024 output tokens per
model call, 30 s per tool, 64 KiB per tool result. Change them with
`limits: { ... }` in `defineAgent` only with a reason.

## Tests

For each tool: input validation, the happy path with the external call stubbed,
an upstream error becoming a thrown Error, and the annotations (assert
`requiresApproval` from `agent.listTools`). Keep `__test__/agent.test.ts`
passing. Never call real providers or models in tests.

## Checking an agent

- `GET /api/v1/agent/tools` lists every tool with `source`, `requiresApproval`
  and `available`. A source that failed to load shows as
  `<source>__unavailable` with `unavailableReason` (usually a missing
  `<SERVICE>_URL`, `<NAME>_MCP_URL` or token).
- The desktop Test agent page shows the same list and each tool step.

## Don'ts

- No hand-written `/agent` routes or model loops outside `init agent`.
- No stdio MCP servers, no `child_process` tools, no shell or arbitrary URL fetch tools.
- No tool without annotations; no write tool marked read-only.
- No secrets anywhere but settings.
