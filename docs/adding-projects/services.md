---
title: Adding Projects - Services
category: Guides
description: Learn how to add and configure services in your application.
---

## Add a Service

A **service** is a standalone application that handles a specific business function. It exposes HTTP endpoints, implements business logic, and can use its own database. Think of it as a specialized worker that does one job well, such as a `user-service` for managing accounts, a `product-service` for catalog management, or an `order-service` for processing purchases.

In ForkLaunch, each service is independent: it can be developed, tested, and deployed separately, making your application modular and scalable.

## Getting Started

You can add a service in two ways:
1. [Using Commands](/docs/adding-projects/services.md#using-commands-recommended)
2. [Manual Creation with Sync](/docs/adding-projects/services.md#manual-creation-with-sync)

### Using Commands (Recommended)

<CodeTabs type="instantiate">
  <Tab title="init">

  ```bash
  forklaunch init service
  ```

  </Tab>
  <Tab title="add">

  ```bash
  forklaunch add service
  ```

  </Tab>
</CodeTabs>

This creates a new service with a complete [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) stack and automatically updates all application artifacts.

### Manual Creation with Sync

If you manually create a service directory, use `forklaunch sync` to register it:

```bash
# After manually creating the service structure
forklaunch sync service email-svc
```

This updates artifacts (manifest, docker-compose, workspace, SDK, tsconfig) to include your manually created service.


### Usage Examples

```bash
# Basic service (see dice roll example)
forklaunch init service roll-dice-svc --path ./my-app/src/modules --database postgresql

# Service with Redis cache
forklaunch init service game-stats-svc --path ./my-app/src/modules --database postgresql --infrastructure redis

# Service with multiple infrastructure
forklaunch init service file-upload-svc --path ./my-app/src/modules --database postgresql --infrastructure redis s3

# Custom path and description
forklaunch init service user-management --path ./user-svc --description "User management service"

# Sync manually created service
forklaunch sync service custom-service --path ./custom-svc
```

For a complete step-by-step example, see the [Dice Roll Example](/docs/examples/dice-roll-node-app.md).

## Learn More

### Service Aliases

The `service` command has several aliases for convenience:
- `svc`
- `project` 
- `proj`

### Init Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--database` | `-d` | The database to use | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `--infrastructure` | `-i` | Add optional infrastructure (can specify multiple) | `redis`, `s3` |
| `--path` | `-p` | The path to initialize the service in | Any valid directory path |
| `--description` | `-D` | The description of the service | Any string |
| `--mappers` |  | Generate mapper files for entity/DTO transformation | Flag (no value) |
| `--dryrun` | `-n` | Dry run the command | Flag (no value) |

### Sync Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--path` | `-p` | The application path | Any valid directory path |
| `--prompts` | `-P` | JSON object with pre-provided answers for service options (see init options) | JSON string |

### Project Structure

A service follows the standard ForkLaunch [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) structure:

```bash
service-name/
├── api/
│   ├── controllers/     # HTTP request handlers
│   └── routes/         # Route definitions
├── domain/
│   ├── enum/           # Enumerations and constants
│   ├── mappers/        # Data transformation logic
│   ├── schemas/        # Validation schemas
│   └── types/          # TypeScript type definitions
├── persistence/
│   ├── entities/       # Database entity definitions
│   ├── seeders/        # Database seed data
│   └── seed.data.ts    # Seed configuration
├── services/
│   └── serviceLogic.ts # Business logic implementation
├── bootstrapper.ts     # Service initialization
├── index.ts           # Service entry point
├── registrations.ts   # Dependency injection setup
├── server.ts          # HTTP server configuration
├── package.json       # Dependencies and scripts
├── mikro-orm.config.ts # Database ORM configuration
├── tsconfig.json      # TypeScript configuration
├── eslint.config.mjs  # ESLint configuration
└── .gitignore         # Git ignore rules
```

### Service Features

By default, you will not need to run any scripts to get going, but if you have added a 
new database, you may need to run `build` and `install` scripts from the top level of the 
application.

Each service includes:
- **Complete [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) Stack**: Routes, Controllers, Services, Interfaces, Domain (schemas/types), Entities, Seeders
- **Database Integration**: Full ORM setup with migrations and seeders
- **API Documentation**: OpenAPI/Swagger documentation generation
- **Validation**: Request/response validation with configurable validators
- **Testing**: Test setup with chosen test framework
- **Docker Support**: Dockerfile and docker-compose integration
- **Infrastructure**: Optional Redis and S3 integration

### Environment Variables

A `.env.local` file is required in your service directory with the following variables:

```bash
# Database configuration (for MikroORM)
DB_NAME=your_database_name
DB_HOST=localhost
DB_PORT=5432
DB_USER=postgres
DB_PASSWORD=postgres

# Server configuration
PORT=3000
NODE_ENV=development

# Infrastructure (if enabled)
REDIS_URL=redis://localhost:6379
AWS_ACCESS_KEY_ID=your_access_key
AWS_SECRET_ACCESS_KEY=your_secret_key
S3_BUCKET_NAME=your_bucket_name
```

**Note**: MikroORM expects individual `DB_*` environment variables, not a single `DATABASE_URL`. The `.env.local` file is used by migration scripts.


### Next Steps

After creating a service:
1. Configure environment variables in `.env.local` (see [Environment Variables](#environment-variables))
2. Define your data model in the entities
3. Implement your business logic in the service layer
4. Set up your API routes and controllers
5. Run migrations
6. Build and Install

For database setup, migrations, building, and running the service, see [Local Development](/docs/local-development.md).

### Common Issues

1. **Database Connection**: Ensure database is running and environment variables are correct
2. **Port Conflicts**: Services use sequential ports (3000, 3001, etc.)
3. **Migration Errors**: Run migrations before starting the service
4. **Infrastructure Dependencies**: Ensure Redis/S3 are configured if enabled
5. **Path Conflicts**: Avoid service names that conflict with existing directories


### Deployment

When you're ready to deploy your service, see [Release](/docs/cli/release.md) and [Deploy](/docs/cli/deploy.md) for information about:
- Deployment commands and workflows
- Default AWS Free Tier resources
- Cost estimates and scaling options

### Best Practices

1. **Single Responsibility**: Keep services focused on one business domain
2. **[RCSIDES Pattern](/docs/artifacts.md#rcsides-architecture-pattern)**: Use the full stack for clean architecture
3. **Error Handling**: Implement comprehensive error handling in service layer
4. **Validation**: Use domain schemas for request/response validation
5. **Testing**: Write tests for business logic and API endpoints
6. **Documentation**: Document API endpoints and business rules
7. **Infrastructure**: Use Redis for caching, S3 for file storage when needed
8. **Free-Tier**: Start with free-tier defaults, upgrade when needed via Platform UI
