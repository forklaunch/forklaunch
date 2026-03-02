---
title: Adding Projects - Routers
category: Guides
description: Learn how to add and configure routers in your application.
---

## Add a Router

A **router** is a set of related API endpoints that you add to an existing service or worker. Unlike services and workers which are standalone projects, routers extend an existing service with new routes, controllers, and business logic. Think of it like adding a special die to your dice set: your service is the basic dice collection (d4, d6, d12, d20), and a router is like adding a custom d20 with special faces—it extends what you can roll (new endpoints) while using the same dice cup (service infrastructure).

In ForkLaunch, routers add a complete [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) stack to your service—new routes, controllers, services, entities, and schemas—all wired into the existing service's server. This lets you organize your API by domain (e.g., a `products-router` for product endpoints, a `users-router` for user endpoints) while keeping everything in one service.

## Getting Started

After you have added a service or worker, you can add a router to it by running the following command:

<CodeTabs type="instantiate">
  <Tab title="init">

  ```bash
  forklaunch init router
  ```

  </Tab>
  <Tab title="add">

  ```bash
  forklaunch add router
  ```
  
  </Tab>
</CodeTabs>

This creates a new router with a complete [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) stack and automatically wires it into your service's `server.ts` and `registrations.ts`.

You will not need to run any new scripts as the router integrates with the existing service configuration.

### Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--path` | `-p` | The service path to initialize the router (must be a service directory) | Path to existing service directory |
| `--infrastructure` | `-i` | Add optional infrastructure (can specify multiple) | `redis`, `s3` |
| `--dryrun` | `-n` | Dry run the command | Flag (no value) |

### Router Aliases

The `router` command has aliases for convenience:
- `controller`
- `routes`

### Project Structure

A router adds the following files to your existing service/worker:

```bash
existing-service/
├── api/
│   ├── controllers/
│   │   └── newRouter.controller.ts # New controller for the router
│   └── routes/
│       └── newRouter.routes.ts     # New route definitions
├── domain/
│   ├── mappers/
│   │   └── newRouter.mapper.ts     # Data transformation logic
│   ├── schemas/
│   │   └── newRouter.schema.ts     # Validation schemas
│   └── types/
│       └── newRouter.types.ts      # TypeScript type definitions
├── persistence/
│   ├── entities/
│   │   └── newRouterRecord.entity.ts # Database entity definition
│   └── seeders/
│       └── newRouterRecord.seeder.ts # Database seeder
├── services/
│   └── newRouter.service.ts        # Business logic implementation
└── interfaces/
    └── newRouter.interface.ts      # Service interface definition
```

Additionally, the router will edit the following files in the service:
- `registrations.ts`: Adds dependency injection configuration
- `server.ts`: Registers the router with the application

### Usage Examples

```bash
# Basic router in current service
forklaunch init router products --path ./my-app/src/modules/my-service

# Router in specific service with path
forklaunch init router users --path ./my-app/src/modules/user-service

# Router with Redis infrastructure
forklaunch init router orders --path ./my-app/src/modules/commerce-service --infrastructure redis

# Router with multiple infrastructure
forklaunch init router files --path ./my-app/src/modules/storage-service --infrastructure redis s3
```

### Next Steps

After adding a router:
1. Define your data model in the entity file
2. Implement your business logic in the service layer
3. Set up your API endpoints in the routes file
4. Configure request/response schemas for validation
5. Run migrations if you modified entities: `pnpm migrate:up` (or `bun migrate:up`)
6. Add test data using the seeder
7. Test the new endpoints
8. Update API documentation

### Common Issues

1. **Service Path**: Ensure the `--path` points to an existing service directory
2. **Route Conflicts**: Check that route paths don't conflict with existing routes
3. **Database Integration**: Verify database configuration matches the parent service
4. **Infrastructure Dependencies**: Ensure Redis/S3 are configured if specified
5. **Dependency Injection**: Router dependencies are automatically registered

### Best Practices

1. **Resource Organization**: Keep routes organized by business domain
2. **RESTful Design**: Use consistent RESTful conventions for endpoints
3. **Validation**: Implement proper request/response validation with schemas
4. **Error Handling**: Use consistent error handling across all routes
5. **Testing**: Write comprehensive tests for all endpoints
6. **Thin Controllers**: Keep controller logic minimal, delegate to services
7. **Infrastructure**: Leverage Redis for caching, S3 for file storage when needed
