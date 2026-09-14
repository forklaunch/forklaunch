---
name: studio
description: "Studio mode: plan and build ForkLaunch apps with a full harness pipeline (optimist/pessimist → review → synthesis → execution). Resumable via S3."
user-invokable: true
---

# ForkLaunch Studio

## When to Use This Skill

Trigger when the user asks to build, generate, scaffold, or create an application. Also trigger on "studio mode" or any request to create a full app from a description.

## How Studio Works

Studio is a **backend-driven** service at `studio-orchestrator`. The planning prompt, harness pipeline, and execution engine live in TypeScript — not in this skill file. This skill describes the flow for reference; the actual prompts are in `@forklaunch-platform/core/prompts/`.

### Pipeline

```text
User prompt
    ↓
PLANNING PHASE (same harness as dashboard chat planning mode)
    ├── Optimist draft (parallel)
    ├── Pessimist draft (parallel)
    ├── Gstack review (CEO, eng, design, devex)
    └── Synthesis → final plan
    ↓
EXECUTION PHASE (step-by-step with checkpoints)
    ├── forklaunch init application
    ├── forklaunch init service × N
    ├── forklaunch init worker × N
    ├── Install dependencies
    └── Verify build
    ↓
COMPLETE (resumable from any point via S3)
```

### Resumability

Every phase and step checkpoints to S3. If the browser disconnects or the user navigates away, the session survives. On reconnect, the frontend loads the session state and resumes from the last checkpoint.

Session state is stored at:
```text
applications/{appId}/studio/sessions/{sessionId}/state.json
```

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/studio-orchestrator/sessions` | Create a new session |
| GET | `/studio-orchestrator/sessions?applicationId=` | List sessions |
| GET | `/studio-orchestrator/sessions/:id` | Get session state |
| POST | `/studio-orchestrator/sessions/:id/stream` | SSE stream (harness + execution) |
| DELETE | `/studio-orchestrator/sessions/:id` | Delete session |

The legacy `/studio-orchestrator/agent/stream` endpoint still works for backward compatibility.

### SSE Events

Studio emits the same event types as the dashboard planning mode:

**Planning phase:**
- `session_state` — full session state on connect (for resume)
- `phase_start` — `{phase: "draft"/"review"/"synthesis"}`
- `text_delta` — `{text, stream: "optimist"/"pessimist"/"final"}`
- `draft_complete` — `{stream, text}`
- `review_complete` — `{review: GstackReview}`
- `plan_complete` — `{text}`

**Execution phase:**
- `phase_start` — `{phase: "execution"}`
- `step_start` — `{step: StudioStep}`
- `step_log` — `{stepId, line}`
- `step_complete` — `{stepId}`
- `step_error` — `{stepId, error}`
- `execution_complete`
- `done`

### Scenarios

Studio handles three scenarios:

1. **Greenfield** — new app from scratch. The harness plans the architecture, then scaffolds via CLI.
2. **Existing frontend** — user has a Next.js/React app, needs a backend. Harness plans backend services, scaffolds into a subdirectory (`--path ./backend`).
3. **Migration** — user has an Express/Nest/Hono backend. Harness analyzes existing code, plans migration, executes incrementally.

### Architecture

```text
@forklaunch-platform/core/
├── prompts/planning.constants.ts    ← shared planning prompt (single source of truth)
├── prompts/harness.constants.ts     ← optimist/pessimist/synthesizer personas
├── prompts/studio.constants.ts      ← scaffold plan tool + execution prompt
└── types/studio.types.ts            ← StudioSession, StudioStep, ScaffoldPlan

studio-orchestrator/
├── studio-harness.service.ts        ← runs the harness pipeline
├── execution-engine.service.ts      ← executes the plan step by step
├── session-store.service.ts         ← S3-backed session persistence + LRU cache
└── session.controller.ts            ← SSE endpoints
```

The planning prompt is shared with `platform-management` (dashboard chat). Both modules import from `@forklaunch-platform/core/prompts/` — never duplicate the prompt.

### Key Rules

1. **All CLI flags are required** — the CLI hangs in interactive mode if any flag is missing. The execution engine discovers flags via `--help`.
2. **Checkpoints are mandatory** — every completed step flushes to S3 before continuing.
3. **Active sessions are never evicted** — LRU eviction only applies to sessions with no SSE connection. ASG scales for concurrent active sessions.
4. **The prompt lives in TypeScript** — not in this skill file. This is proprietary IP. The skill describes behavior; the prompt drives it.
