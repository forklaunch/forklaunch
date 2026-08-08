# AWS RDS TLS trust in generated apps

Apps scaffolded by `forklaunch init application` trust the AWS RDS certificate
authorities out of the box, with **full certificate verification** — nothing
disables `rejectUnauthorized` anywhere.

How it works:

- `src/modules/core/certs/aws-rds-global-bundle.pem` is written into every
  generated app (the public [AWS RDS global trust bundle](https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem),
  CAs valid to 2061).
- The generated Dockerfile copies it into the image and appends it to the
  runtime trust store:

  ```dockerfile
  COPY core/certs/aws-rds-global-bundle.pem /etc/ssl/certs/aws-rds-global-bundle.pem
  ENV NODE_EXTRA_CA_CERTS=/etc/ssl/certs/aws-rds-global-bundle.pem
  ```

  `NODE_EXTRA_CA_CERTS` appends to (never replaces) the default trust store.
  It is honored by Node and by Bun >= 1.2.3 (earlier Bun versions read only
  the first certificate from a multi-cert bundle).
- The generated `mikro-orm.config.ts` treats `DB_SSL=true` as TLS with full
  verification, honors `DB_SCHEMA` (default `public`) for shared-database
  tiers, and uses a per-service migrations table
  (`mikro_orm_migrations_<service>`), so multiple services can share one
  database with per-app schemas.

## Migrating existing apps

Apps generated before CLI 1.5.0 need three things:

1. Copy the bundle into your repo (path is convention, not requirement):

   ```sh
   curl -o src/modules/core/certs/aws-rds-global-bundle.pem \
     https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem
   ```

2. Add the two lines above to your Dockerfile (after source files are copied,
   before the app runs). Adjust the `COPY` source path to your Docker build
   context — it is `core/certs/...` when the context is the modules directory.

3. Ensure your DB config does not set `rejectUnauthorized: false`; with the
   bundle in place, `DB_SSL=true` verifies RDS certificates fully.
