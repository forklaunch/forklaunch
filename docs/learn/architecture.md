## Project Structure

```bash
.
│
├── docker-compose.yaml
│   # Services: tempo, loki, prometheus, grafana, otel-collector
│   # LGTM Stack (Loki, Grafana, Tempo, Mimir) for observability
│
└── src/modules/ #modules/
    │
    ├── package.json # Root workspace config
    ├── pnpm-workspace.yaml # Monorepo workspace definition for node projects
    ├── tsconfig.json # TypeScript root config
    ├── tsconfig.base.json # Shared TS config
    ├── vitest.config.ts # Root test config
    ├── eslint.config.mjs # Root linting config
    ├── LICENSE # MIT License
    ├── README.md # Project documentation
    ├── Dockerfile # Container build config
    │
    ├── assets/
    │   └── logo.svg # ForkLaunch logo
    │
    ├── patches/
    │   └── @jercle__yargonaut.patch # Dependency patch
    │
    ├── core/
    │   ├── package.json
    │   ├── index.ts # Main entry point
    │   ├── rbac.ts # Role-Based Access Control
    │   ├── registrations.ts # Dependency injection registrations
    │   ├── tsconfig.json
    │   ├── vitest.config.ts
    │   ├── eslint.config.mjs
    │   │
    │   └── persistence/
    │       ├── index.ts
    │       └── sql.base.entity.ts #  Base SQL entity
    │
    │   # Purpose: Core library with shared foundational infrastructure
    │   # Dependencies: MikroORM, ForkLaunch packages, Express, Zod
    │
    ├── monitoring/
    │   ├── package.json
    │   ├── index.ts # Main entry point
    │   ├── metricsDefinitions.ts # Custom metrics
    │   ├── otel-collector-config.yaml # OpenTelemetry config
    │   ├── prometheus.yaml # Prometheus config
    │   ├── tempo.yaml # Tempo tracing config
    │   ├── tsconfig.json
    │   ├── vitest.config.ts
    │   ├── eslint.config.mjs
    │   │
    │   └── grafana-provisioning/
    │       ├── dashboards/
    │       │   ├── application-overview.json
    │       │   ├── default.yaml
    │       │   └── red.json
    │       └── datasources/
    │           └── datasources.yaml
    │
    │   # Purpose: Monitoring library for metrics, logs, and traces
    │   # Dependencies: ForkLaunch core
    │
    └── universal-sdk/
        ├── package.json
        ├── index.ts # Main entry point
        ├── universalSdk.ts # SDK implementation
        ├── tsconfig.json
        ├── vitest.config.ts
        └── eslint.config.mjs
        │
        # Purpose: Universal SDK for shared utilities
        # Dependencies: ForkLaunch common & universal-sdk
```