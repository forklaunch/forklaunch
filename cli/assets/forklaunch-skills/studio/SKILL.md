---
name: studio
description: "Studio mode: fast app generation for 3 scenarios (greenfield, existing Next.js, backend migration). Triggers on build/generate/scaffold/create app/studio."
user-invokable: true
---

# ForkLaunch Studio

## When to Use This Skill

Trigger when the user asks to build, generate, scaffold, or create an application. Also trigger on "studio mode" or any request to create a full app from a description.

## CRITICAL: Speed Rules

1. **Scaffold first, customize second.** Use `forklaunch init` for ALL structural work (services, routers, workers). Never manually create service directory trees.
2. **Batch CLI commands.** Run multiple `forklaunch init service` commands sequentially in one bash call using `&&`.
3. **Don't ask permission for obvious decisions.** If the user says "build a food delivery app," you know it needs restaurants, orders, delivery, users. Just scaffold them.
4. **Implement in parallel where possible.** Entities across different services have no dependencies. Write them all, then write schemas, then services, then controllers.
5. **Use `--variant base` for services** unless the user specifically needs IAM (`iam-better-auth`) or billing (`billing-stripe`).
6. **Every CLI command MUST include ALL flags** or it hangs in interactive mode. See the CLI skill for flag reference.

## Scenario Detection

Ask ONE question to determine the scenario:

> "Are we starting from scratch, adding a backend to an existing frontend, or migrating an existing backend?"

Then follow the appropriate path below. If the user's request makes the scenario obvious, skip the question.

---

## Scenario 1: Greenfield (Nothing Exists)

This is the default. Build everything from scratch.

### Step 1: Initialize Application (~30 seconds)

```bash
forklaunch init application <app-name> \
  --path . \
  --modules-path src/modules \
  --database postgresql \
  --runtime node \
  --validator zod \
  --http-framework express \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest \
  --license MIT \
  --author "<author>" \
  --description "<description>"
```

### Step 2: Scaffold All Services (~2 minutes)

Identify the domain services from the user's description. Common patterns:

| App Type | Typical Services |
|----------|-----------------|
| Marketplace | users, listings, orders, payments, reviews, messaging |
| SaaS | users, workspaces, projects, billing, notifications |
| Healthcare | patients, providers, appointments, records, billing |
| Delivery | restaurants, orders, delivery, drivers, payments, reviews |
| E-commerce | products, cart, orders, payments, shipping, reviews |

Scaffold them all in one pass:

```bash
forklaunch init service restaurants --path . --database postgresql --runtime node --variant base && \
forklaunch init service orders --path . --database postgresql --runtime node --variant base && \
forklaunch init service delivery --path . --database postgresql --runtime node --variant base
```

For workers:
```bash
forklaunch init worker dispatch-worker --path . --runtime node
```

### Step 3: Implement Domain Logic

For EACH service, implement in this order (files can be written in parallel across services):

1. **Entity** at `src/modules/<service>/persistence/entities/<name>.entity.ts`
2. **Schema** at `src/modules/<service>/domain/schemas/<name>.schema.ts`
3. **Service** at `src/modules/<service>/domain/services/<name>.service.ts`
4. **Controller** at `src/modules/<service>/api/controllers/<name>.controller.ts`
5. **Routes** at `src/modules/<service>/api/routes/<name>.routes.ts`
6. **Wire into** `registrations.ts` and `bootstrapper.ts`
7. **Export controller** from `api/controllers/index.ts`

### Step 4: Migrations and Verify

```bash
cd src/modules/<service> && pnpm migrate:create && pnpm migrate:up
```

Then `pnpm dev` to verify.

---

## Scenario 2: Existing Next.js Frontend, Adding ForkLaunch Backend

The user already has a Next.js (or React/Vue/Svelte) app. They need a backend.

### Step 1: Initialize ForkLaunch Alongside the Frontend

> **Warning:** Do NOT use `--path .` when an existing frontend lives at the project root — it will scaffold backend files into the frontend directory. Always use a dedicated subdirectory like `--path ./backend`.

```bash
# From the project root, initialize ForkLaunch in a backend subdirectory
forklaunch init application <app-name> \
  --path ./backend \
  --modules-path src/modules \
  --database postgresql \
  --runtime node \
  --validator zod \
  --http-framework express \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest \
  --license MIT \
  --author "<author>" \
  --description "<description>"
```

### Step 2: Scaffold Backend Services

Same as greenfield. Identify domains from the existing frontend's API calls or data model.

Look at the existing frontend for clues:
- `fetch('/api/...')` calls tell you what endpoints exist
- State management (Redux, Zustand) slices tell you what entities exist
- Route structure tells you what pages need data

### Step 3: Generate SDK for Frontend Consumption

```bash
forklaunch sdk generate
```

This generates a typed client at `src/modules/client-sdk/`.

### Step 4: Wire Frontend to SDK

Replace existing fetch calls with SDK calls:

**Before:**
```typescript
const res = await fetch('/api/restaurants', { headers });
const data = await res.json();
```

**After:**
```typescript
import { createClient } from '@<app-name>/client-sdk';

const api = createClient({ baseUrl: process.env.NEXT_PUBLIC_API_URL });
const token = await getToken();
if (!token) return; // redirect to login or handle gracefully

const result = await api.restaurants.list({
  headers: { Authorization: `Bearer ${token}` }
});
if (result.code === 200) {
  const restaurants = result.response;
  // use restaurants
}
```

### Step 5: Configure CORS

In each service's `bootstrapper.ts`, add the frontend origin to CORS config.

### Step 6: Share Auth

Use `SHARED_SESSION_SCHEMA` from `@<app-name>/core`. Configure JWT with the same JWKS URL the frontend uses. If the frontend uses a different auth provider, create adapter middleware.

---

## Scenario 3: Existing Backend, Migrating to ForkLaunch

The user has an Express/Nest/Hono/FastAPI/Rails backend they want to migrate.

### Step 1: Analyze Existing Backend

Before scaffolding, read the existing codebase to understand:
- What routes exist (map to services)
- What database models exist (map to entities)
- What auth strategy is used (map to IAM variant)
- What external integrations exist (Stripe, SendGrid, etc.)

### Step 2: Initialize and Scaffold

Same as greenfield, but service names and structure should mirror the existing backend's domain boundaries.

### Step 3: Migrate One Service at a Time

Pick the simplest service first. For each:

**3a. Port entities:**

Prisma model:
```prisma
model Restaurant {
  id        String   @id @default(uuid())
  name      String
  address   String
  isActive  Boolean  @default(true)
  createdAt DateTime @default(now())
}
```

Becomes:
```typescript
import { defineEntity, p } from '@<app-name>/core';
import { sqlBaseProperties } from '@<app-name>/core';

export const RestaurantEntity = defineEntity({
  name: 'RestaurantEntity',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    address: p.string(),
    isActive: p.boolean().default(true),
  },
});
```

For sensitive data, use `defineComplianceEntity` with `fp` instead of `defineEntity` with `p`.

**3b. Port route handlers:**

Old Express:
```typescript
app.get('/restaurants/:id', auth, async (req, res) => {
  const restaurant = await prisma.restaurant.findUnique({ where: { id: req.params.id } });
  res.json(restaurant);
});
```

New ForkLaunch:
```typescript
handlers.get(schemaValidator, '/restaurants/:id', {
  name: 'GetRestaurant',
  summary: 'Get restaurant by ID',
  access: 'protected',
  auth: { allowedRoles: ['admin', 'user'] },
  params: IdSchema,
  responses: { 200: RestaurantResponseSchema },
}, async (req, res) => {
  const restaurant = await restaurantService.getById(req.params.id);
  res.status(200).json(restaurant);
});
```

**3c. Port business logic** into service files. Extract from route handlers.

**3d. Run migrations** to create ForkLaunch's schema. If sharing the same database, create new tables alongside existing ones.

### Step 4: Parallel Run During Migration

Run old and new backends simultaneously. Use a reverse proxy (nginx, Caddy) or API gateway to route:
- Migrated routes to ForkLaunch
- Unmigrated routes to old backend

### Step 5: Decommission Old Backend

Once all routes are migrated, verified, and tested, shut down the old backend.

---

## Speed Patterns

### Batch Entity Creation

Write all entities for a service at once. They rarely depend on each other.

### Batch Schema Creation

Write all schemas after all entities. Schemas reference entity shapes but are separate files.

### Template for Rapid Controller

Every controller follows the same structure. When building multiple endpoints, copy this pattern and modify:

```typescript
import { handlers, schemaValidator } from '@<app-name>/core';

export const createRestaurantController = (
  restaurantService: RestaurantService,
) => [
  handlers.get(schemaValidator, '/', {
    name: 'ListRestaurants',
    summary: 'List all restaurants',
    access: 'authenticated',
    responses: { 200: array(RestaurantResponseSchema) },
  }, async (req, res) => {
    const restaurants = await restaurantService.list(req.session);
    res.status(200).json(restaurants);
  }),

  handlers.get(schemaValidator, '/:id', {
    name: 'GetRestaurant',
    summary: 'Get restaurant by ID',
    access: 'authenticated',
    params: IdSchema,
    responses: { 200: RestaurantResponseSchema },
  }, async (req, res) => {
    const restaurant = await restaurantService.getById(req.params.id);
    res.status(200).json(restaurant);
  }),

  handlers.post(schemaValidator, '/', {
    name: 'CreateRestaurant',
    summary: 'Create a new restaurant',
    access: 'protected',
    auth: { allowedRoles: ['admin'] },
    body: CreateRestaurantSchema,
    responses: { 201: RestaurantResponseSchema },
  }, async (req, res) => {
    const restaurant = await restaurantService.create(req.body, req.session);
    res.status(201).json(restaurant);
  }),

  handlers.put(schemaValidator, '/:id', {
    name: 'UpdateRestaurant',
    summary: 'Update a restaurant',
    access: 'protected',
    auth: { allowedRoles: ['admin'] },
    params: IdSchema,
    body: UpdateRestaurantSchema,
    responses: { 200: RestaurantResponseSchema },
  }, async (req, res) => {
    const restaurant = await restaurantService.update(req.params.id, req.body);
    res.status(200).json(restaurant);
  }),

  handlers.delete(schemaValidator, '/:id', {
    name: 'DeleteRestaurant',
    summary: 'Delete a restaurant',
    access: 'protected',
    auth: { allowedRoles: ['admin'] },
    params: IdSchema,
    responses: { 204: {} },
  }, async (req, res) => {
    await restaurantService.delete(req.params.id);
    res.status(204).send();
  }),
];
```

### Don't Over-Engineer v1

For initial scaffolding:
- Skip pagination until asked
- Skip filtering until asked
- Skip caching until asked
- Skip WebSockets until asked
- Use simple CRUD, add complexity when the user asks for it
- Use `access: 'authenticated'` as default, tighten later

### When the User Says "All the Bells and Whistles"

They mean feature-complete CRUD, not every possible optimization. Build:
- Full CRUD for each entity
- Proper auth on each route
- Relational queries (restaurant has many menu items, order has many items)
- Status enums and transitions (order: pending, confirmed, preparing, delivering, delivered)
- Basic search/filter on list endpoints

They do NOT mean (unless they ask):
- Real-time WebSocket updates
- Complex caching strategies
- Rate limiting
- Analytics dashboards
- Email notifications
- Background job queues
