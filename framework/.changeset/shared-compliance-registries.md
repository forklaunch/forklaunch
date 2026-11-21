---
'@forklaunch/core': patch
---

Retention enforcement finds the entities' retention policies again. The compliance, retention and user-id-field registries are now shared across the package's separately bundled entry points (`persistence`, `services`, CommonJS and ES builds); before, `RetentionService` read an empty copy and every run reported "No retention policies registered".
