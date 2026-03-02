---
title: Local Development
category: Guides
description: Learn how to run your ForkLaunch application locally.
---

## Local Development

### Getting Started
Once you have generated a new `ForkLaunch` application with at least one service or worker, you can test your application locally with the following steps: 



## Initial Setup

Run these commands to set up your development environment:

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  # Install dependencies
  pnpm install

  # Build the application
  pnpm build

  # Set up database and initial migrations
  forklaunch database:setup
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  # Install dependencies
  bun install

  # Build the application
  bun run build

  # Set up database and initial migrations
  forklaunch database:setup
  ```

  </Tab>
</CodeTabs>

**Note**: The `database:setup` script is a convenience command that:
1. Starts Docker containers (PostgreSQL, Redis, etc.)
2. Initializes the migrations folder (`migrate:init`)
3. Runs all migrations (`migrate:up`)
4. Seeds the database (`seed`)

If you prefer to run these steps manually, see the [Database Migrations](#database-migrations) section below.

## Development Mode

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

## Production Mode

Run the application in production mode:

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

When you modify entities or add new services/workers, you'll need to create and run migrations:

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  # Initialize migrations folder (first time only, creates migrations/ directory)
  pnpm migrate:init

  # Create a new migration after modifying entities
  pnpm migrate:create

  # Run pending migrations
  pnpm migrate:up

  # Rollback last migration
  pnpm migrate:down
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  # Initialize migrations folder (first time only, creates migrations/ directory)
  bun migrate:init

  # Create a new migration after modifying entities
  bun migrate:create
  
  # Run pending migrations
  bun migrate:up

  # Rollback last migration
  bun migrate:down
  ```

  </Tab>
</CodeTabs>


**Important**: 
- `migrate:init` creates the migrations folder and initial migration (run once per service)
- `migrate:create` creates a new migration file based on entity changes
- Migrations require a `.env.local` file with `DB_*` environment variables (see [Environment Variables](#environment-variables))

## Environment Variables

Create a `.env.local` file in your service directory for database configuration:

```bash
# Database configuration (required for migrations)
DB_NAME=your_database_name
DB_HOST=localhost
DB_PORT=5432
DB_USER=postgres
DB_PASSWORD=postgres

# Server configuration
PORT=3000
NODE_ENV=development
```

**Note**: MikroORM expects individual `DB_*` environment variables, not a single `DATABASE_URL`. The migration scripts use `DOTENV_FILE_PATH=.env.local` to load these variables.

## Troubleshooting

When adding new services or workers, you might encounter initial setup errors. If this happens, try these steps in order:

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  # Ensure Docker containers are running
  docker-compose up -d

  # Initialize and run migrations
  pnpm migrate:init
  pnpm migrate:up

  # If errors persist, rebuild the development environment
  pnpm build
  pnpm dev
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  # Ensure Docker containers are running
  docker-compose up -d

  # Initialize and run migrations
  bun migrate:init
  bun migrate:up
  
  # If errors persist, rebuild the development environment
  bun run build
  bun dev
  ```

  </Tab>
</CodeTabs>

ForkLaunch is designed to be modular, allowing you to add new components incrementally without breaking existing functionality. These commands help ensure new components are properly integrated into your application.
