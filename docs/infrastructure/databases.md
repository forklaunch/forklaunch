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

## Environment Variables

Database connections are configured via environment variables:

```bash
# Connection URL (preferred)
DATABASE_URL=postgresql://user:password@localhost:5432/myapp

# Or individual components
DB_HOST=localhost
DB_PORT=5432
DB_USER=user
DB_PASSWORD=password
DB_NAME=myapp
```

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
