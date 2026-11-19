---
'@forklaunch/core': minor
'@forklaunch/express': minor
'@forklaunch/hyper-express': minor
---

The `cors` application option accepts `'environment'`, which states that the application takes its allowed origins from `CORS_ORIGINS` (the same behaviour as leaving the option out). Creating an application with no `cors` option is now marked `@deprecated`, so a linter with a no-deprecated rule (`typescript/no-deprecated` in oxlint, `@typescript-eslint/no-deprecated`) points at every application that has not said how it answers cross-origin browser requests since the 4.0 default changed. To clear it, pass `cors: 'environment'` or `cors: { origin: [...] }`. Nothing changes at runtime.
