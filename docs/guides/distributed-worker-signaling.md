---
title: Distributed Worker Signaling
category: Advanced Guides
description: Architectural patterns for cancelling jobs across distributed worker instances using a shared signal store.
---

# Distributed Worker Signaling Patterns

## Problem

In a distributed worker system (e.g., BullMQ, Postgres, Kafka), any worker instance can receive an API request (like "cancel this job"), but the job may be actively running on a *different* worker. The API handler has no direct access to the process running on another machine.

You cannot:
- Kill a process on another machine from your machine
- Remove a job from the queue if another worker has already locked it (BullMQ throws "Job could not be removed because it is locked by another worker")
- Rely on in-memory state (like a `Set` or `Map`) because each worker has its own memory space

## Solution: Shared Store Signal + Worker-Side Polling

Use a shared data store (Redis, DynamoDB, database) as a signaling mechanism between workers.

### Architecture

```
                    ┌─────────────────┐
                    │   API Request   │
                    │ "Cancel Job X"  │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  Worker A       │
                    │  (receives req) │
                    │                 │
                    │  1. Set flag in │
                    │     Redis:      │
                    │     job:X:cancel│
                    │     = true      │
                    │                 │
                    │  2. Try local   │
                    │     kill (no-op │
                    │     if not here)│
                    └─────────────────┘

                    ┌─────────────────┐
                    │  Redis / Shared │
                    │     Store       │
                    │                 │
                    │  job:X:cancel   │
                    │  = true         │
                    │  (TTL: 5 min)   │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  Worker B       │
                    │  (owns Job X)   │
                    │                 │
                    │  Polling loop   │
                    │  every 2s:      │
                    │  "Is job:X      │
                    │   cancelled?"   │
                    │                 │
                    │  → Yes → SIGKILL│
                    │    the process  │
                    └─────────────────┘
```

### Key Design Decisions

#### 1. Signal, Don't Command

The cancel request writes a **flag** to a shared store. It does not try to directly stop the job. The worker that owns the job is responsible for checking the flag and acting on it.

This decouples the "request to cancel" from the "act of cancelling."

#### 2. Poll, Don't Subscribe

Use polling (e.g., every 2 seconds) rather than pub/sub for cancellation checks. Reasons:
- **Reliability**: If a pub/sub message is missed (network blip, worker restart), the cancellation is lost forever. A flag in Redis persists and will be found on the next poll.
- **Simplicity**: No subscription management, no message ordering concerns.
- **Distributed consistency**: Every worker instance checks the same source of truth.

The polling interval is a tradeoff between responsiveness and load. 2 seconds is a good default for most use cases.

#### 3. TTL on Flags

Always set a TTL on cancellation flags (e.g., 5 minutes). If the job completes or the worker dies before reading the flag, you don't want stale flags accumulating.

#### 4. Multi-Layer Cancellation

Handle cancellation at multiple points in the job lifecycle:

| Job State | Cancellation Strategy |
|---|---|
| **Queued** (waiting/delayed) | Remove from queue directly; no lock conflict |
| **Active** (locked by worker) | Set Redis flag → worker polls and kills process |
| **Between steps** | Check in-memory flag at step boundaries (optimization) |

**Never** try to `job.remove()` on an active/locked job; the queue system will reject it. Only remove jobs that are in a non-locked state (waiting, delayed, paused).

#### 5. Local Process + Remote Signal

The cancel handler should do both:
1. **Set the remote signal** (Redis flag): handles the cross-worker case
2. **Try to kill locally**: handles the same-worker case with zero latency

```
async cancelJob(jobId):
  // Always set the distributed signal
  await redis.set(`job:${jobId}:cancel`, 'true', 'EX', 300)

  // Also try local kill (fast path if we own it)
  const localProcess = activeProcesses.get(jobId)
  if (localProcess):
    localProcess.kill('SIGKILL')
    return true

  // Not local; rely on the polling mechanism on the owning worker
  return true
```

#### 6. Don't Conflate Queue States with Execution States

Queue states (waiting, active, completed, failed) are managed by the queue system. Execution states (running, cancelling, cancelled) are your application's concern. Keep them separate:

- Use the queue system for job lifecycle (enqueue, dequeue, retry)
- Use your shared store for execution signals (cancel, pause)
- Use your database for durable status tracking (deployment status)

### Implementation Checklist

- [ ] Cancellation flag written to shared store with TTL
- [ ] Active process polling loop checks shared store every N seconds
- [ ] Local process kill attempted as fast path
- [ ] Queue removal only attempted for non-locked job states (waiting, delayed, paused)
- [ ] Cancellation checked at step boundaries in multi-step jobs
- [ ] Flag cleaned up after job completes or is cancelled
- [ ] Graceful handling of "flag not found" (treat as not cancelled)

### Anti-Patterns

- **In-memory-only cancellation tracking**: Only works on the same worker instance. Breaks with horizontal scaling.
- **Trying to remove locked/active jobs from the queue**: Queue systems lock active jobs. Attempting removal throws errors.
- **Pub/sub without persistence**: If the owning worker misses the message, the job runs to completion.
- **Synchronous cancellation**: Don't wait for confirmation that the job was killed. Set the signal and return. The owning worker handles it asynchronously.
- **No TTL on flags**: Stale flags can cause false cancellations on future jobs if IDs are reused.
