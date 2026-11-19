---
'@forklaunch/core': patch
---

Moves `@forklaunch/opentelemetry-instrumentation-hyper-express` to 0.0.6, which no longer depends on `@opentelemetry/core` 1.x (GHSA-8988-4f7v-96qf, unbounded memory allocation in W3C Baggage propagation).
