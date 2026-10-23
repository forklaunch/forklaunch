---
"@forklaunch/blueprint-iam-base": patch
"@forklaunch/blueprint-iam-better-auth": patch
---

IAM surfacing signs the full mounted HMAC path. With `@forklaunch/core` verifying the full wire path, remote `surfaceRoles`/`surfacePermissions` calls signed over `/:id/surface-*` were rejected with 403. iam-base now signs `/user/<id>/surface-{roles,permissions}` (`surfacingPath`), and iam-better-auth's `authorizationScopePath` includes the `/user` mount. Better Auth IAM also builds on hyper-express, registers Better Auth's routes as a delegated surface, expires invitations after 48 hours (Better Auth takes seconds), maps Google `given_name`/`family_name` to the user's names, and resolves the Google callback URL the way Better Auth does. Shipped to new apps through the CLI templates; these packages are not published.
