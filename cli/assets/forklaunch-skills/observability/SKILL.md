---
name: observability
description: "Observability: OTel wiring, metrics definitions, security events, alert rules, notifiers, issues, retention of telemetry."
user-invokable: true
---

# ForkLaunch Platform Observability

## When to Use This Skill

Use when the user asks about:

- Adding or emitting metrics (OpenTelemetry counters/gauges/histograms)
- The monitoring module (`metricsDefinitions.ts`, collector configs)
- Alert rules, alert evaluation, notifier configs (Slack/email)
- Issues (auto-created incidents) and issue ranking
- Security-event alerting
- Logs/traces/metrics querying (Prometheus, Loki, Tempo)

## Architecture

```
service code ──otel.getMetric(...)──▶ OTLP ──▶ otel-collector ──▶ Prometheus (:8889)
        │                                            ├──▶ Loki (logs)
        │ PinoLogger (auto log mirror)               └──▶ Tempo (traces)
        ▼
observability-api worker (BullMQ) schedules two evaluators on separate cadences;
both feed one shared Issue → Notifier pipeline:
  - every ALERT_EVALUATION_INTERVAL_MS ─ AlertEvaluationService.evaluateAll()
    (PromQL via MonitoringService)
  - every MONITORING_CAPACITY_INTERVAL_MS ─ MonitoringCapacityService.evaluateAll()
    (per-container memory via CloudWatch Logs Insights + ECS DescribeTaskDefinition)
  ─▶ Issue dedup/escalate/resolve ──▶ NotifierService (Slack webhook + SES email)
```

- **Automatic per-request telemetry** comes from `@forklaunch/core`'s
  `forklaunchExpress` (HTTP/Express instrumentation, `http_requests_total`,
  `http_request_duration_ms`, `http_errors_total`, log mirroring with
  trace/span/correlation IDs). Every module gets this by constructing
  `forklaunchExpress(SchemaValidator(), otelCollector, …)` — it is a
  framework default, not per-endpoint work.
- Collector/backends config lives in `src/modules/monitoring/`
  (`otel-collector-config.yaml`, `prometheus.yaml`, `loki.yaml`,
  `tempo.yaml`, `grafana-provisioning/`).

## Adding a Metric

1. Declare it in `src/modules/monitoring/metricsDefinitions.ts`
   (`metricsDefinitions({ myThing: 'counter' | 'gauge' | 'histogram' | ... })`).
2. Every module already registers
   `OtelCollector: new OpenTelemetryCollector(OTEL_SERVICE_NAME, OTEL_LEVEL, metrics)`
   as a Singleton in `registrations.ts` — inject it and emit:

```typescript
otel.getMetric('myThing').add(1, { organization_id: orgId });
```

3. For best-effort emission in failure-prone paths, wrap in try/catch or use
   a tolerant helper (see `recordSecurityEvent` in
   `monitoring/securityEvents.ts`, or the counter helpers in
   `deployment-agent-worker/domain/services/substrate-bootstrap.service.ts`).
4. To alert on it, extend `buildPromQL` in
   `observability-api/domain/services/alert-evaluation.service.ts` — the
   evaluator **skips unknown `metricName` values**, so a metric is not
   alertable until it has a PromQL mapping.

## Alert Rules & Evaluation

- Entity: `observability-api/persistence/entities/alert-rule.entity.ts` —
  `organizationId`, `appId`, `serviceId?`, `env`, `metricName`, `operator`
  (GT/LT/GTE/LTE), `value`, `windowSize` (5m/15m/1h/6h/24h), `severity`
  (ERROR/ALERT/INCIDENT), `notifierConfigId?`, `enabled`.
- `AlertEvaluationService.evaluateAll()` runs from the BullMQ worker on an
  interval; rules evaluate ascending by severity so escalation works; only
  `enabled` rules evaluate; `notifierConfigId` (when set) selects the
  notifier config, else service-specific falls back to org-wide.
- Built-in metric names: `error_rate`, `latency_p95`, `request_rate`
  (HTTP-duration-derived), plus `security_events` /
  `security_events:<event_type>` for the security counter.
- Breaches dedupe into an `Issue` per `{org, app, service, env}`, append
  `IssueEvidence`, escalate severity, auto-resolve on clear, publish over WS,
  and notify (cooldown via `issue.lastNotifiedAt` +
  `NOTIFIER_ESCALATION_COOLDOWN_MS`).
- Failed evaluation jobs land in the DB-backed DLQ
  (`alert-evaluation-dlq`).

## Notifiers

`NotifierConfig` (per-service or org-wide fallback): Slack incoming webhook
URL and/or email (SES). Delivery is `Promise.allSettled` — transport
failures are logged, never thrown. Tests:
`observability-api/__test__/notifier.service.test.ts` (mock fetch/SES) —
follow that pattern for delivery assertions.

## Security-Event Alerting

Taxonomy + emission: see the `security` skill and
`docs/security-events.md`. Default high-risk rules (device-code replay,
tenant-isolation violation, HMAC failure bursts, rate-limit bursts,
deploy-without-approval) are seeded idempotently via the
security-alert-defaults service in observability-api.

## Telemetry Data Retention

Issue/IssueEvidence carry `retention` declarations (90d, delete,
`RETENTION_DAYS_ISSUES` override); DLQ and event records have short delete
policies. Full inventory: `docs/data-retention.md`. Enforcement runs daily
per module via `retention:enforce` (ECS scheduled task).

## Querying

- `MonitoringService.queryPromQL` (Prometheus), `LogsService` (Loki),
  `TracesService` (Tempo), CloudWatch bridge in
  `cloudwatch-logs.service.ts`.
- CLI: `forklaunch observe` for logs/metrics/traces/live health.

## Gotchas

- OTel metric names pass through the Prometheus exporter's naming transform —
  when writing PromQL, verify the exported series name (check existing
  `buildPromQL` cases) rather than assuming the camelCase definition name.
- The alert evaluator loads **all** rules globally per run — keep per-rule
  work O(1 PromQL query) and avoid unbounded label cardinality
  (never label metrics with unbounded values like raw IPs or tokens).
- `monitoring` is a library module (no server) — it must stay
  dependency-light; only type-level imports from `@forklaunch/core`.
