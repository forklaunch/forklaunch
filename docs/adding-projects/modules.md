---
title: Adding Projects - Modules
category: Guides
description: Learn how to add and configure modules in your application.
---

## Add a Module

A **module** is a preconfigured, production-ready service that provides common functionality out of the box. Unlike regular services that you build from scratch, modules come with complete implementations for complex features like authentication (`iam-base`, `iam-better-auth`) and billing (`billing-base`, `billing-stripe`). Think of modules as ready-to-use building blocks that save you weeks of development time.

In ForkLaunch, modules are complete services with full RCSIDES stacks, database integration, and API documentation, all configured and ready to use. You can customize them later using `forklaunch eject` if needed.

## Getting Started


### Using Commands (Recommended)

To add a module to your application, run the following command:

<CodeTabs type="instantiate">
  <Tab title="init">

  ```bash
  forklaunch init module
  ```

  </Tab>
  <Tab title="add">

  ```bash
  forklaunch add module
  ```

  </Tab>
</CodeTabs>



`ForkLaunch` will automatically add the module to your workspace, configure Docker Compose, and register any necessary scripts.

By default, you will not need to run any scripts to get going, but if your module depends on external services or configuration, you may need to run `build` and `install` scripts from the top level of the application.

### Available Modules

| Module ID | Description | Purpose |
| :-------- | :---------- | :------ |
| `billing-base` | App hooks only | Base billing infrastructure with extensible payment provider interface |
| `billing-stripe` | Stripe billing implementation | Complete Stripe integration with webhook handling and payment flow |
| `iam-base` | Authorization only | Base Identity and Access Management with JWT support and custom auth methods |
| `iam-better-auth` | Better auth implementation for IAM | BetterAuth integration for advanced authentication features |

### Init Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--module` | `-m` | The module type to initialize | `billing-base`, `billing-stripe`, `iam-base`, `iam-better-auth` |
| `--database` | `-d` | The database to use | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `--path` | `-p` | The application path to initialize the module in | Any valid directory path |
| `--dryrun` | `-n` | Dry run the command | Flag (no value) |

### Module Structure

Modules are generated as complete service implementations with the following structure:

```bash
module-name/
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
├── bootstrapper.ts     # Service initialization
├── index.ts           # Service entry point
├── registrations.ts   # Dependency injection setup
├── sdk.ts             # Service SDK for internal use
├── server.ts          # HTTP server configuration
├── package.json       # Dependencies and scripts
├── mikro-orm.config.ts # Database ORM configuration
└── eslint.config.mjs   # ESLint configuration
```

### Module Features

Each module includes:
- **Complete API**: RESTful endpoints with OpenAPI documentation
- **Database Integration**: Entities, migrations, and seeders
- **Authentication**: Built-in auth middleware and JWT support
- **Validation**: Request/response validation with Zod or TypeBox
- **Testing**: Test setup with Vitest or Jest
- **Docker Support**: Dockerfile and docker-compose integration
- **Type Safety**: Full TypeScript support with generated types

### Environment Variables

Each module requires specific environment variables:

```bash
# Database configuration (all modules)
DATABASE_URL=postgresql://user:password@localhost:5432/dbname
DATABASE_TYPE=postgresql

# IAM modules (iam-base, iam-better-auth)
JWT_SECRET=your-jwt-secret-key
SESSION_SECRET=your-session-secret

# Billing modules (billing-base, billing-stripe)
# Stripe-specific (billing-stripe only)
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...

# Cache configuration (billing modules)
REDIS_URL=redis://localhost:6379
```

Check the generated `.env.example` file for complete variable lists.

### Usage Examples

#### Adding a Billing Module
```bash
# Interactive mode
forklaunch init module --path ./my-app

# Direct specification
forklaunch init module billing --path ./my-app --module billing-stripe --database postgresql
```

#### Adding an IAM Module  
```bash
# Base IAM with JWT
forklaunch init module auth --path ./my-app --module iam-base --database postgresql

# BetterAuth implementation
forklaunch init module auth --path ./my-app --module iam-better-auth --database postgresql
```

### Next Steps

After creating a module:
1. Configure environment variables in `.env.local`
2. Run database migrations: `npm run migrate:up`
3. Seed initial data: `npm run seed`
4. Start the module: `npm run dev`
5. Review the generated API documentation at `/docs`
6. Test the endpoints using the OpenAPI interface
7. Integrate with other services using the generated SDK

### Common Issues

1. **Module Conflicts**: Only one module of each type (IAM/Billing) can be added per application
2. **Database Connection**: Ensure your database is running and environment variables are correct
3. **Port Conflicts**: Modules use sequential ports starting from 3000
4. **Migration Errors**: Run `npm run migrate:up` before starting the module
5. **Cache Dependencies**: Billing modules require Redis to be running

### Best Practices

1. **Choose the Right Implementation**: Use base modules for custom integrations, specific modules (Stripe, BetterAuth) for faster setup
2. **Environment Management**: Keep environment variables in `.env.local` for development
3. **Database Migrations**: Always run migrations after adding a module
4. **API Documentation**: Use the generated OpenAPI docs for integration
5. **Testing**: Test module endpoints before building dependent services
6. **Monitoring**: Enable logging and monitoring for production deployments
