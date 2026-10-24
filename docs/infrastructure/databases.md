---
title: "Databases"
description: "Supported databases, ORM integration with MikroORM, entity patterns, and migration workflows."
category: "Infrastructure"
---

## Overview

ForkLaunch uses [MikroORM](https://mikro-orm.io/) as its database abstraction layer, supporting 8 database engines out of the box. The CLI configures your chosen database during project initialization, generates entities with the correct base class, and sets up migration tooling.

## Supported Databases

| Database | CLI Flag | Driver Package | Notes |
|----------|----------|----------------|-------|
| PostgreSQL | `postgresql` | `@mikro-orm/postgresql` | Recommended for production |
| MySQL | `mysql` | `@mikro-orm/mysql` | Full support |
| MariaDB | `mariadb` | `@mikro-orm/mariadb` | Full support |
| MongoDB | `mongodb` | `@mikro-orm/mongodb` | Document store, no SQL migrations |
| Microsoft SQL Server | `mssql` | `@mikro-orm/mssql` | Enterprise environments |
| SQLite | `sqlite` | `@mikro-orm/sqlite` | File-based, good for prototyping |
| better-sqlite3 | `better-sqlite3` | `@mikro-orm/better-sqlite` | Faster SQLite driver |
| libSQL | `libsql` | `@mikro-orm/libsql` | Turso-compatible SQLite fork |

## Selecting a Database

Choose your database when initializing an application:

```bash
forklaunch init application my-app --database postgresql
```

Or when adding a new service:

```bash
forklaunch init service billing --database postgresql
```

The CLI generates the MikroORM configuration, entity base classes, and migration scripts for your chosen database.

## Entity Pattern

All entities extend `SqlBaseEntity` from `@{{app-name}}/core`, which provides `id`, `createdAt`, and `updatedAt` fields:

```typescript
import { SqlBaseEntity } from '@{{app-name}}/core';
import { Entity, Property, ManyToOne } from '@mikro-orm/core';

@Entity()
export class Application extends SqlBaseEntity {
  @Property()
  name!: string;

  @Property({ type: 'text', nullable: true })
  description?: string;

  @Property({ type: 'json', nullable: true })
  settings?: Record<string, unknown>;

  @ManyToOne(() => Organization)
  organization!: Organization;
}
```

### Key Entity Rules

- **Always extend `SqlBaseEntity`**: provide UUID `id`, `createdAt`, `updatedAt`
- **JSON fields** use `@Property({ type: 'json', nullable: true })`
- **Enums** use `const` objects, not TypeScript `enum`:

```typescript
const ApplicationStatus = {
  ACTIVE: 'active',
  ARCHIVED: 'archived'
} as const;
type ApplicationStatus = (typeof ApplicationStatus)[keyof typeof ApplicationStatus];

@Property()
status!: ApplicationStatus;
```

## Service Pattern

Services receive an `EntityManager` via their params object and return entities directly, never DTOs:

```typescript
async createApplication(
  params: { em: EntityManager } & CreateApplicationParams
): Promise<Application> {
  const { em, name, organizationId } = params;

  const org = await em.findOneOrFail(Organization, { id: organizationId });
  const app = em.create(Application, { name, organization: org });
  await em.flush();

  return app;
}
```

### Key Service Rules

- **`em.flush()`** after all mutations: MikroORM uses Unit of Work pattern
- **Return entities**, not DTOs: controllers handle mapping
- **Fork the EntityManager** for each request via dependency injection

## Migrations

ForkLaunch uses pnpm scripts for all migration operations. Never run raw MikroORM CLI commands directly.

### Creating a Migration

```bash
cd src/modules/my-service
pnpm migrate:create
```

This generates a timestamped migration file in the `migrations/` directory.

### Running Migrations

```bash
# Apply all pending migrations
pnpm migrate:up

# Rollback the last migration
pnpm migrate:down
```

### Migration Best Practices

1. **Always use `pnpm migrate:*` scripts**, they handle configuration loading
2. **Review generated migrations** before running: auto-generated SQL may need adjustment
3. **Test migrations** in your integration tests using `BlueprintTestHarness` with `useMigrations: true`
4. **Keep migrations small**, one logical change per migration

## Connection Settings

Each service's `mikro-orm.config.ts` reads its connection from environment variables. Which ones depends on the engine.

| Engine | Connection | Deployed on ForkLaunch |
|--------|------------|------------------------|
| PostgreSQL, MySQL, MariaDB, SQL Server | `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME` (separate settings) | Amazon RDS |
| MongoDB | `DB_URL` when it is set; otherwise a URL built from `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME`. The database is always `DB_NAME` | Amazon DocumentDB |
| SQLite, better-sqlite3, libSQL | `DB_NAME` (the file) | nothing is provisioned |

### Locally

`forklaunch init` writes the separate settings into the service's `.env.local`, pointing at the database container in `docker-compose.yaml`:

```bash
DB_NAME=my-app-billing-dev
DB_HOST=localhost
DB_USER=postgresql
DB_PASSWORD=postgresql
DB_PORT=5432
```

There is no `DB_URL` locally. A MongoDB service builds its URL from those values for the single-node replica set in docker-compose:

```
mongodb://<DB_USER>:<DB_PASSWORD>@<DB_HOST>:<DB_PORT>/<DB_NAME>?authSource=admin&directConnection=true&replicaSet=rs0
```

### Deployed

The platform sets `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD` and `DB_NAME` for every engine, and `DB_URL` in the scheme of the engine's driver:

| Engine | `DB_URL` |
|--------|----------|
| PostgreSQL | `postgresql://...` |
| MySQL, MariaDB | `mysql://...` |
| SQL Server | `mssql://...` |
| MongoDB | `mongodb://...?tls=true&replicaSet=rs0&readPreference=secondaryPreferred&retryWrites=false&authSource=admin` |

The SQL engines connect with the separate settings, as locally. `DB_SSL` (default: on outside `development`) turns on TLS with full certificate verification, and on PostgreSQL `DB_SCHEMA` (default `public`) selects the schema on shared tiers.

MongoDB runs on Amazon DocumentDB, which a hand-built URL cannot reach. DocumentDB needs:

- `tls=true`. The certificate is signed by the Amazon RDS CA, which the generated `Dockerfile` adds to Node's trust store (`NODE_EXTRA_CA_CERTS=/etc/ssl/certs/aws-rds-global-bundle.pem`).
- `retryWrites=false`. DocumentDB does not support retryable writes, and the driver turns them on by default.
- `replicaSet=rs0` and `authSource=admin`. The master user lives in `admin`, and the cluster is a replica set named `rs0`.

The platform's `DB_URL` already carries all of these, so the config uses it as is:

```ts
const mikroOrmOptionsConfig = defineConfig({
  // Deployed, the platform sets DB_URL (Amazon DocumentDB needs tls=true,
  // replicaSet=rs0 and retryWrites=false); locally, without it, the URL is
  // built from DB_HOST/DB_PORT for the docker-compose replica set.
  clientUrl:
    getEnvVar('DB_URL') ||
    `mongodb://${validConfigInjector.resolve(
    tokens.DB_USER
  )}:${validConfigInjector.resolve(
    tokens.DB_PASSWORD
  )}@${validConfigInjector.resolve(
    tokens.DB_HOST
  )}:${validConfigInjector.resolve(
    tokens.DB_PORT
  )}/${validConfigInjector.resolve(
    tokens.DB_NAME
  )}?authSource=admin&directConnection=true&replicaSet=rs0`,
  // The platform's DB_URL names the database in its path; dbName from DB_NAME
  // is set anyway, so a DB_URL without a path still selects the right one.
  dbName: validConfigInjector.resolve(
    tokens.DB_NAME
  ),
  // ...
});
```

If you point a MongoDB service at your own database, set `DB_URL` to a full connection string; it wins over the separate settings.

### Switching engines

`forklaunch change service --database <engine>` (and `change worker`) rewrites `mikro-orm.config.ts` for you. Switching to `mongodb` writes the `clientUrl` with the `DB_URL` fallback and `dbName`; switching away from `mongodb` removes `clientUrl` and writes back `dbName`, `host`, `user`, `password` and `port`.

### Upgrading an existing MongoDB service

A MongoDB service created before this change builds its URL from the separate settings only, so deployed on DocumentDB it connects without TLS and with retryable writes, and fails. The required fix is one line in each MongoDB service's (and worker's) `mikro-orm.config.ts`.

Before:

```ts
  clientUrl: `mongodb://${validConfigInjector.resolve(
```

After:

```ts
  clientUrl: getEnvVar('DB_URL') || `mongodb://${validConfigInjector.resolve(
```

A service switched to MongoDB with `forklaunch change` may resolve `'DB_USER'` rather than `tokens.DB_USER` on the next line; the edit is the same: put `getEnvVar('DB_URL') ||` in front of the template string. `getEnvVar` is already imported from `@forklaunch/core/services` at the top of the file. That line is all that is required: the platform's `DB_URL` names the service's database in its path. Optionally, to match newly generated services, also add `dbName` right after the `clientUrl` property (after the line ending in ``?authSource=admin&directConnection=true&replicaSet=rs0`,``). It selects `DB_NAME` even for a `DB_URL` without a database path:

```ts
  dbName: validConfigInjector.resolve(
    tokens.DB_NAME
  ),
```

Locally nothing changes: without `DB_URL` the service builds the same URL as before.

## Entity Relationships

MikroORM supports all standard relationship types:

```typescript
// One-to-Many
@OneToMany(() => Deployment, d => d.application)
deployments = new Collection<Deployment>(this);

// Many-to-One
@ManyToOne(() => Application)
application!: Application;

// Many-to-Many
@ManyToMany(() => Tag)
tags = new Collection<Tag>(this);
```

The platform uses these relationships extensively. For example:

- `Organization` → `Application` (one-to-many)
- `Application` → `Environment` (one-to-many)
- `Environment` → `Deployment` (one-to-many)

## Testing with Databases

Use `BlueprintTestHarness` to run integration tests against real database containers:

```typescript
import { BlueprintTestHarness } from '@forklaunch/testing';

const harness = new BlueprintTestHarness({
  getConfig: async () => {
    const { default: config } = await import('../mikro-orm.config');
    return config;
  },
  databaseType: 'postgres',
  useMigrations: false // Schema generation is faster for tests
});

const setup = await harness.setup();
// setup.orm is ready; run your tests
await harness.cleanup();
```

See the [Testing guide](/docs/guides/testing.md) for full details on database testing patterns.

## Related Documentation

- [Infrastructure Overview](/docs/infrastructure/overview.md)
- [Caches](/docs/infrastructure/caches.md)
- [Testing Guide](/docs/guides/testing.md)
- [CLI Commands](/docs/guides/cli-commands.md)
