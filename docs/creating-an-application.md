---
title: Creating an Application
category: Guides
description: Step-by-step guide to building a real multi-service application with ForkLaunch, from scaffold to production deploy.
---

## Overview

In [Getting Started](/docs/getting-started.md) you proved out the ForkLaunch core loop: install the CLI, scaffold an app, write an endpoint, hit it locally. This guide shows you how to build a multi-system application.

You'll construct a commerce application (`users`, `products`, and `orders` services) to understand how ForkLaunch scales across multiple teams and deployables. Along the way you'll see how service isolation works, how services communicate with full TypeScript type safety, how background work gets offloaded to workers, and how to go from a working local stack to a versioned production deploy.

By the end of this guide you should understand not just *what* to run, but *why* ForkLaunch is structured the way it is.

## Step 1: Initialize Your Application

Run `forklaunch init app` from the directory where you want the project to live:

```bash
forklaunch init app my-app
cd my-app
```

ForkLaunch will prompt you for runtime, HTTP framework, validator, database, and test framework. All choices can be changed later with `forklaunch change`.

To skip prompts and use flags directly (useful in scripts or AI-assisted workflows):

```bash
forklaunch init app my-app \
  --path . \
  --o "src/moduels" \
  --database postgresql \
  --validator zod \
  --http-framework express \
  --runtime node
```

For all supported flags (`--test-framework`, `--modules`, `--description`, `--author`, `--license`, `--dryrun`), see [CLI Reference: init](/docs/cli/init.md).

**What gets generated:**
- Application structure under `src/modules`
- `.forklaunch/manifest.toml`: the source of truth for your project configuration
- `docker-compose.yaml` for local development
- Base configuration and tsconfig files
- Git repository initialization

For a detailed look at the generated directory structure, see [Architecture](/docs/learn/architecture.md).

The `manifest.toml` file is worth understanding from the start. It's the single source of truth for your entire project: every service, worker, library, and configuration choice lives there. When you run a `forklaunch` command, it reads and updates the manifest, then regenerates the affected files. This is why you should use CLI commands to make structural changes rather than editing generated files directly: the manifest is what keeps everything in sync. However, if you find yourself copying in a lucky library or making manual changes then rung `forklaunch sync` periodically to keep your manifes inline with your repo.

> **Note:** During initialization you will be prompted about preconfigured modules. These are optional ready-made service blueprints. See [Preconfigured Services](/docs/preconfigured-services.md) for details.

## Step 2: Create Your First Service

```bash
forklaunch init service users
```

This generates a complete service with routes, controllers, services, entities, and mappers, all wired together and registered in `docker-compose.yaml` automatically.

A service in ForkLaunch is a unit of deployment and ownership. The `users` service will own everything about users: its database schema, business logic, and API surface. When you add `products` and `orders` in Step 4, each gets its own isolated database, its own container, and its own entry point. This boundary is intentional: it means you can scale, redeploy, or rewrite any service independently without touching the others. It also gives teams a clear ownership model: the team that owns `users` controls the `users` service end-to-end.

## Step 3: Define Your API

The generated service includes example routes. Customize them in `src/modules/users/api/routes/user.routes.ts`.

The key thing to notice about ForkLaunch route definitions is that the route *is* the schema. The `body`, `params`, and `responses` you declare aren't documentation that can drift; they're enforced at runtime via your validator (Zod, TypeBox, etc.) and used to automatically generate your OpenAPI spec. Your API contract lives in one place, and your docs and runtime validation are both derived from it.

```typescript
import { forklaunchExpress } from '@forklaunch/express';
import { SchemaValidator, string } from '@forklaunch/validator/zod';

const validator = SchemaValidator();
const app = forklaunchExpress(validator, openTelemetryCollector);

app.post('/users', {
  name: 'CreateUser',
  summary: 'Create a new user',
  body: {
    email: string,
    name: string
  },
  responses: {
    201: {
      id: string,
      email: string,
      name: string
    },
    400: { error: string }
  }
}, async (req, res) => {
  const user = await createUser(req.body);
  res.status(201).json(user);
});

app.get('/users/:id', {
  name: 'GetUser',
  summary: 'Get user by ID',
  params: { id: string },
  responses: {
    200: { id: string, email: string, name: string },
    404: { error: string }
  }
}, async (req, res) => {
  const user = await getUser(req.params.id);
  if (!user) return res.status(404).json({ error: 'User not found' });
  res.json(user);
});

export default app;
```

## Step 4: Add More Services

```bash
forklaunch init service products
forklaunch init service orders
```

Each service gets its own isolated database, routes, and `docker-compose.yaml` entry. ForkLaunch updates your manifest, spins up a new database container, and wires the whole thing together automatically.

The `orders` service is a good example of why isolation matters: an order needs to know about both users and products, but it shouldn't own that data. It should ask the `users` and `products` services for what it needs, get a typed answer, and combine the results. This is what Step 5 sets up.

## Step 5: Service-to-Service Communication

Without tooling, service-to-service calls are brittle: you hardcode URLs, manually construct request bodies, and have no compile-time guarantee that the other service's API hasn't changed. ForkLaunch solves this with the Universal SDK: a typed client that's generated from the OpenAPI spec each service publishes at runtime.

Services import each other as TypeScript libraries with full type safety via the Universal SDK. In the orders service `registrations.ts`:

```typescript
import { universalSdk } from '@forklaunch/universal-sdk';
import type { ProductsSdkClient } from '@my-app/products';
import type { UsersSdkClient } from '@my-app/users';

const runtimeDependencies = environmentConfig.chain({
  ProductsSdk: {
    lifetime: Lifetime.Singleton,
    type: type<Promise<ProductsSdkClient>>(),
    factory: ({ PRODUCTS_URL }) =>
      universalSdk<ProductsSdkClient>({
        host: PRODUCTS_URL,
        registryOptions: { path: 'api/v1/openapi' }
      })
  },
  UsersSdk: {
    lifetime: Lifetime.Singleton,
    type: type<Promise<UsersSdkClient>>(),
    factory: ({ USERS_URL }) =>
      universalSdk<UsersSdkClient>({
        host: USERS_URL,
        registryOptions: { path: 'api/v1/openapi' }
      })
  }
});
```

Then in your service layer:

```typescript
export async function createOrder(
  input: CreateOrderInput,
  productsService: ProductsSdkClient,
  usersService: UsersSdkClient
) {
  const user = await usersService.getUser({ params: { id: input.userId } });
  const product = await productsService.getProduct({ params: { id: input.items[0].productId } });
  // ...
}
```

The TypeScript compiler enforces contracts between services. If the `users` service changes a response shape, every dependent service shows a compile error before you deploy, not a runtime failure in production. This is the core promise of the Universal SDK: the same type system that checks your local code also checks cross-service contracts. See [Framework Reference: Universal SDK](/docs/framework/universal-sdk.md) for full details.

## Step 6: Add a Worker

```bash
forklaunch init worker email-notifications --type bullmq
```

Workers exist because some tasks shouldn't block the request that triggered them. Sending a welcome email, generating a PDF, resizing an image: these can take seconds, fail, and need to retry. Doing that work inside a service request means the user waits, and a transient failure becomes a user-visible error.

Workers pull jobs off a queue (BullMQ over Redis in this case) so your service responds immediately and the work happens in the background. ForkLaunch generates the worker with the queue connection, job handlers, retry configuration, and Docker container already wired up; you define the job logic, not the plumbing. See [Adding Projects: Workers](/docs/adding-projects/workers.md) for configuration options.

## Step 7: Start Local Development

<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  pnpm install
  pnpm dev
  ```

  </Tab>
  <Tab title="bun">

  ```bash
  bun install
  bun run dev
  ```

  </Tab>
</CodeTabs>

ForkLaunch starts all services with hot reload, using the generated `docker-compose.yaml` to spin up databases, caches, and queues automatically. No manual database configuration needed. Each service runs in its own container alongside its own database, the same topology you'll have in production, running locally.

## Step 8: Test the API

```bash
# Create a user
curl -X POST http://localhost:3000/users \
  -H "Content-Type: application/json" \
  -d '{"email":"user@example.com","name":"John Doe"}'

# Get user by ID
curl http://localhost:3000/users/<id>
```

## Step 9: Export API Documentation

```bash
forklaunch openapi export --all
```

This generates OpenAPI 3.0 specs in the `openapi/` directory; ready for Swagger UI, Redoc, or any API documentation tool. See [CLI Reference: openapi](/docs/cli/openapi.md).

ForkLaunch generates the spec from the same route definitions you wrote in Step 3. You don't maintain a separate YAML file; the spec is a derived artifact, always in sync with your code. This command snapshots it to disk so you can publish it to an API portal, share it with a frontend team, or feed it into a code generator.

## Step 10: Connect to the Platform

Local development is fully self-contained, but the ForkLaunch platform is what unlocks deployments, release tracking, and environment management. Linking your local project to a platform application is how the deploy commands in Steps 11 and 12 know where to send your code.

1. Visit [https://forklaunch.com/dashboard](https://forklaunch.com/dashboard) and create an application.
2. Copy the Application ID.
3. Link your local project:

```bash
forklaunch login
forklaunch integrate --app <your-application-id>
```

## Step 11: Create a Release

```bash
git add .
git commit -m "Initial implementation"

forklaunch release create --version 1.0.0
```

A release in ForkLaunch is an immutable snapshot of your application at a specific version. It's what you deploy: not a branch, not a commit hash, but a named and versioned artifact. This separation between "what exists in git" and "what is deployed" is intentional: it lets you roll back to a previous release without untangling git history, and it gives your operations team a clear audit trail of what ran where and when. See [CLI Reference: release](/docs/cli/release.md) for tagging and notes options.

## Step 12: Deploy

```bash
# Deploy to staging
forklaunch deploy create \
  --release 1.0.0 \
  --environment staging \
  --region us-east-1

# Deploy to production after testing
forklaunch deploy create \
  --release 1.0.0 \
  --environment production \
  --region us-east-1
```

The two-step deploy (staging first, then production) is a pattern ForkLaunch encourages by design. Both environments use identical infrastructure: the same ECS task definitions, RDS configuration, and ALB setup. The only difference is the environment name. This parity means that if it works in staging, it works in production, and promotion is a deliberate, auditable act rather than a hope.

ForkLaunch deploys to your own AWS account using standard services (ECS, RDS, ElastiCache, ALB); you own the infrastructure and the data. See [CLI Reference: deploy](/docs/cli/deploy.md) for all options.

## Notes

- Use `forklaunch change` to modify project configuration. If you edit files directly, run `forklaunch sync` afterward to keep the manifest up to date.
- Preconfigured modules use opaque business logic by default. Use `forklaunch eject` to take ownership of the implementation.
- MikroORM provides entity manager access and raw SQL when needed.

## Next Steps

- [Adding Projects](/docs/adding-projects.md): Services, workers, libraries, and routers
- [Local Development](/docs/local-development.md): docker-compose, hot reload, and environment setup
- [Framework Reference](/docs/framework.md): HTTP, validation, telemetry, and authorization
- [Customization](/docs/customization.md): Adapt generated patterns to your conventions
