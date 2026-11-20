---
'@forklaunch/express': minor
'@forklaunch/hyper-express': minor
---

`forklaunchExpress` is now a constant typed by the exported `ForklaunchExpressFactory` interface, and the deprecation for calls without a `cors` option sits on that interface's call signature. A no-deprecated lint rule now reports only those calls. Before, it also reported every import, re-export and `typeof forklaunchExpress`, so an application's `core/registrations.ts` re-export failed lint. Calling the factory is unchanged.
