---
title: Local Development
category: Guides
description: Run your ForkLaunch application locally with docker-compose, hot reload, and database migrations.
---

## Overview

When you generate a ForkLaunch application, everything needed to run locally is included: a `docker-compose.yaml` that starts all required infrastructure (databases, caches, queues) and per-service scripts for migrations, hot reload, and seeding. No manual database configuration is needed.

## Initial Setup

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  # Install dependencies
  pnpm install

  # Initialize and run database migrations for all services
  pnpm database:setup
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  # Install dependencies
  bun install

  # Initialize and run database migrations for all services
  bun database:setup
  ```

  </Tab>
</CodeTabs>

**Note**: Your can use `pnpm database:setup` or`bun database:setup` that will:
1. Start Docker containers (PostgreSQL, Redis, etc.)
2. Initialize the migrations folder (`migrate:init`)
3. Run all migrations (`migrate:up`)
4. Seed the database (`seed`)

If you prefer to run these steps manually, see the [Database Migrations](#database-migrations) section below.

## Start Development Mode

Run the application with hot reloading enabled:

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  pnpm dev
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  bun dev
  ```

  </Tab>
</CodeTabs>

This starts all services with hot reload. File changes are picked up automatically; restart individual services without rebuilding the whole stack.

## Production Mode

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  pnpm start
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  bun start
  ```

  </Tab>
</CodeTabs>

## Database Migrations

When you add new services or modify entities, create and run a new migration:

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  # Initialize the migrations folder (run once per new service)
  pnpm migrate:init

  # Create a migration after modifying entities
  pnpm migrate:create

  # Apply pending migrations
  pnpm migrate:up

  # Roll back the last migration
  pnpm migrate:down
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  # Initialize the migrations folder (run once per new service)
  bun migrate:init

  # Create a migration after modifying entities
  bun migrate:create

  # Apply pending migrations
  bun migrate:up

  # Roll back the last migration
  bun migrate:down
  ```

  </Tab>
</CodeTabs>

- `migrate:init`: creates the migrations folder and initial migration file. Run once per service after it is first generated.
- `migrate:create`: generates a new migration file from the current entity diff.
- `migrate:up` / `migrate:down`: apply or roll back pending migrations.

## Environment Variables

Create a `.env.local` file in each service directory with database credentials. MikroORM uses individual `DB_*` variables rather than a connection string:

```bash
DB_NAME=my_app
DB_HOST=localhost
DB_PORT=5432
DB_USER=postgres
DB_PASSWORD=postgres

PORT=3000
NODE_ENV=development
```

Migration scripts use `DOTENV_FILE_PATH=.env.local` to load these values automatically.

## Troubleshooting

If you see errors after adding a new service, try these steps:

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  # Make sure Docker containers are running
  docker-compose up -d

  # Initialize and apply migrations for the new service
  pnpm migrate:init
  pnpm migrate:up

  # Rebuild if necessary
  pnpm build && pnpm dev
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  # Make sure Docker containers are running
  docker-compose up -d

  # Initialize and apply migrations for the new service
  bun migrate:init
  bun migrate:up

  # Rebuild if necessary
  bun run build && bun dev
  ```

  </Tab>
</CodeTabs>

ForkLaunch is designed to be modular. You can add new services and workers incrementally without touching existing components.

## Next Steps

- [Adding Projects](/docs/adding-projects.md): Add more services, workers, and libraries
- [Environment variables](/docs/cli/environment.md): Manage environment variables with the CLI
- [Customization](/docs/customization.md): Adapt generated patterns to your conventions
