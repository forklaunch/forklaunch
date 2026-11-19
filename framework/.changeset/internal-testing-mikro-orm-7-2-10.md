---
'@forklaunch/internal': patch
'@forklaunch/testing': patch
---

Republishes `@forklaunch/internal` and `@forklaunch/testing` on `@mikro-orm/core` 7.2.10. Their published versions (1.2.35 and 1.3.0) still pinned 7.2.5 while `@forklaunch/core` 4.0.0 pins 7.2.10, so an application got two copies of MikroORM and its entity types stopped matching (`EntitySchemaWithMeta ... is not assignable to EntityName`).
