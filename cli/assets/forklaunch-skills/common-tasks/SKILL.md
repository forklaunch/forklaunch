---
name: common-tasks
description: "Guides: add endpoints, create entities, add pages, feature gating, debugging, migrations."
user-invokable: true
---

# ForkLaunch Common Tasks

## When to Use This Skill

Use for step-by-step guides to common development tasks.

## Task 1: Add a New API Endpoint (Full Stack)

### Step 1: Define Schema

```typescript
// domain/schemas/widget.schema.ts
import {
  string,
  number,
  optional,
  array,
  enum_,
  date,
  record,
} from "@{{app-name}}/core";
import { WidgetStatusEnum } from "../enum/widget-status.enum";

export const WidgetSchemas = {
  CreateWidgetSchema: {
    name: string,
    description: optional(string),
    type: string,
    applicationId: string,
  },

  UpdateWidgetSchema: {
    name: optional(string),
    description: optional(string),
    status: optional(enum_(WidgetStatusEnum)),
  },

  WidgetResponseSchema: {
    id: string,
    name: string,
    description: optional(string),
    type: string,
    status: enum_(WidgetStatusEnum),
    applicationId: string,
    createdAt: date,
    updatedAt: date,
  },
};
```

### Step 2: Create Enum (if needed)

```typescript
// domain/enum/widget-status.enum.ts
export const WidgetStatusEnum = {
  ACTIVE: "active",
  INACTIVE: "inactive",
  ERROR: "error",
} as const;
export type WidgetStatusEnum =
  (typeof WidgetStatusEnum)[keyof typeof WidgetStatusEnum];
```

### Step 3: Create Entity

```typescript
// persistence/entities/widget.entity.ts
import { Entity, Property, ManyToOne, Enum } from "@mikro-orm/core";
import { SqlBaseEntity } from "@{{app-name}}/core";
import { WidgetStatusEnum } from "../../domain/enum/widget-status.enum";
import { Application } from "./application.entity";

@Entity()
export class Widget extends SqlBaseEntity {
  @Property({ index: true })
  name!: string;

  @Property({ type: "text", nullable: true })
  description?: string;

  @Property()
  type!: string;

  @Enum({ items: () => WidgetStatusEnum })
  status!: WidgetStatusEnum;

  @ManyToOne("Application")
  application!: Application;
}
```

Add to entity index: `persistence/entities/index.ts`

### Step 4: Create Migration

```bash
pnpm migrate:create
```

Then edit the generated migration file in `migrations-postgresql/`.

### Step 5: Implement Service

```typescript
// domain/services/widget.service.ts
import { EntityManager } from "@mikro-orm/core";
import { Widget, Application } from "../../persistence/entities";
import { WidgetStatusEnum } from "../enum/widget-status.enum";

export class WidgetService {
  async listWidgets(params: {
    organizationId: string;
    applicationId?: string;
    em: EntityManager;
  }): Promise<Widget[]> {
    const { organizationId, applicationId, em } = params;
    const where: Record<string, unknown> = { application: { organizationId } };
    if (applicationId)
      where.application = {
        ...(where.application as object),
        id: applicationId,
      };
    return em.find(Widget, where, { orderBy: { createdAt: "DESC" } });
  }

  async createWidget(params: {
    data: {
      name: string;
      description?: string;
      type: string;
      applicationId: string;
    };
    organizationId: string;
    em: EntityManager;
  }): Promise<Widget> {
    const { data, organizationId, em } = params;
    const app = await em.findOneOrFail(Application, {
      id: data.applicationId,
      organizationId,
    });
    const widget = em.create(Widget, {
      ...data,
      application: app,
      status: WidgetStatusEnum.ACTIVE,
    });
    em.persist(widget);
    return widget;
  }

  async getWidget(params: {
    id: string;
    em: EntityManager;
  }): Promise<Widget | null> {
    return params.em.findOne(
      Widget,
      { id: params.id },
      { populate: ["application"] },
    );
  }

  async updateWidget(params: {
    id: string;
    data: Partial<{
      name: string;
      description: string;
      status: WidgetStatusEnum;
    }>;
    em: EntityManager;
  }): Promise<Widget | null> {
    const { id, data, em } = params;
    const widget = await em.findOne(Widget, { id });
    if (!widget) return null;
    em.assign(widget, data);
    return widget;
  }

  async deleteWidget(params: {
    id: string;
    organizationId: string;
    em: EntityManager;
  }): Promise<void> {
    const { id, organizationId, em } = params;
    const widget = await em.findOneOrFail(
      Widget,
      { id },
      { populate: ["application"] },
    );
    if (widget.application.organizationId !== organizationId)
      throw new Error("Access denied");
    em.remove(widget);
  }
}
```

### Step 6: Create Controller

```typescript
// api/controllers/widget.controller.ts
import {
  handlers,
  schemaValidator,
  string,
  optional,
  array,
} from "@{{app-name}}/core";
import { ci, tokens } from "../../bootstrapper";
import {
  JWKS_PUBLIC_KEY_URL,
  PLATFORM_EDITOR_ROLES,
  PLATFORM_VIEWER_ROLES,
} from "../../constants";
import { WidgetSchemas } from "../../domain/schemas/widget.schema";

const widgetFactory = ci.scopedResolver(tokens.WidgetService);
const emFactory = ci.scopedResolver(tokens.EntityMgr);

export const listWidgets = handlers.get(
  schemaValidator,
  "/",
  {
    name: "List Widgets",
    summary: "Get all widgets",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    query: { applicationId: optional(string) },
    responses: { 200: array(WidgetSchemas.WidgetResponseSchema), 401: string },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const results = await widgetFactory().listWidgets({
      organizationId: req.session.organizationId,
      applicationId: req.query.applicationId,
      em,
    });
    res.status(200).json(results);
  },
);

export const createWidget = handlers.post(
  schemaValidator,
  "/",
  {
    name: "Create Widget",
    summary: "Create a new widget",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_EDITOR_ROLES,
    },
    body: WidgetSchemas.CreateWidgetSchema,
    responses: {
      201: WidgetSchemas.WidgetResponseSchema,
      401: string,
      403: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const result = await widgetFactory().createWidget({
      data: req.body,
      organizationId: req.session.organizationId,
      em,
    });
    await em.flush();
    res.status(201).json(result);
  },
);

export const getWidget = handlers.get(
  schemaValidator,
  "/:id",
  {
    name: "Get Widget",
    summary: "Get widget by ID",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    params: { id: string },
    responses: {
      200: WidgetSchemas.WidgetResponseSchema,
      401: string,
      404: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const result = await widgetFactory().getWidget({ id: req.params.id, em });
    if (!result) {
      res.status(404).send("Widget not found");
      return;
    }
    res.status(200).json(result);
  },
);

export const updateWidget = handlers.patch(
  schemaValidator,
  "/:id",
  {
    name: "Update Widget",
    access: "protected",
    params: { id: string },
    body: WidgetSchemas.UpdateWidgetSchema,
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_EDITOR_ROLES,
    },
    responses: {
      200: WidgetSchemas.WidgetResponseSchema,
      401: string,
      404: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const result = await widgetFactory().updateWidget({
      id: req.params.id,
      data: req.body,
      em,
    });
    if (!result) {
      res.status(404).send("Widget not found");
      return;
    }
    await em.flush();
    res.status(200).json(result);
  },
);

export const deleteWidget = handlers.delete(
  schemaValidator,
  "/:id",
  {
    name: "Delete Widget",
    access: "protected",
    params: { id: string },
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_EDITOR_ROLES,
    },
    responses: { 204: string, 401: string, 404: string },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    await widgetFactory().deleteWidget({
      id: req.params.id,
      organizationId: req.session.organizationId,
      em,
    });
    await em.flush();
    res.status(204).send("Deleted");
  },
);
```

### Step 7: Export from controller index

```typescript
// api/controllers/index.ts
export {
  listWidgets,
  createWidget,
  getWidget,
  updateWidget,
  deleteWidget,
} from "./widget.controller";
```

### Step 8: Create Route

```typescript
// api/routes/widget.routes.ts
import { forklaunchRouter, schemaValidator } from "@{{app-name}}/core";
import { ci, tokens } from "../../bootstrapper";
import {
  listWidgets,
  createWidget,
  getWidget,
  updateWidget,
  deleteWidget,
} from "../controllers/widget.controller";

const otel = ci.resolve(tokens.OpenTelemetryCollector);
const widgetRouter = forklaunchRouter("/widgets", schemaValidator, otel);

export const listWidgetsRoute = widgetRouter.get("/", listWidgets);
export const createWidgetRoute = widgetRouter.post("/", createWidget);
export const getWidgetRoute = widgetRouter.get("/:id", getWidget);
export const updateWidgetRoute = widgetRouter.patch("/:id", updateWidget);
export const deleteWidgetRoute = widgetRouter.delete("/:id", deleteWidget);
```

### Step 9: Register in server.ts

Mount the router in `server.ts`.

### Step 10: Register service in DI

Add `WidgetService` to `registrations.ts`:

```typescript
WidgetService: {
  lifetime: Lifetime.Scoped,
  type: WidgetService,
  factory: () => new WidgetService()
}
```

### Step 11: Run migration and test

```bash
pnpm migrate:up
pnpm test
pnpm dev
```

## Task 2: Add Feature Gating to an Endpoint

### Backend

**Never call `billingCacheService.getCachedFeatures(orgId)` directly** — the
raw cache returns `null` on miss and silently treats paid orgs as
featureless. Always import the surfacing function from `bootstrapper`:

```typescript
import { FEATURE_FLAGS } from "@{{app-name}}/core";
import { surfaceFeatures } from "../../bootstrapper";

// In controller handler:
const features = await surfaceFeatures({ organizationId });
if (!features.has(FEATURE_FLAGS.CUSTOM_DOMAINS)) {
  return res
    .status(403)
    .send("Custom domains require a Pro subscription. Please upgrade.");
}
```

For plan-limit checks, use `surfaceSubscription({ organizationId, sub })`
the same way. See `backend-patterns` → "Feature Gating & Billing Surfacing"
for the full pattern and why direct cache reads are a bug.

### Frontend

```typescript
import { useFeatureAccess } from "@/hooks/use-feature-access"
import { FeatureGate } from "@/components/billing/feature-gate"

// Conditional rendering
function MyComponent() {
  const { checkFeature } = useFeatureAccess()
  const { hasAccess, showUpgradeModal } = checkFeature('CUSTOM_DOMAINS')

  if (!hasAccess) {
    return <Button onClick={showUpgradeModal}>Upgrade to Pro</Button>
  }
  return <CustomDomainConfig />
}

// Or use FeatureGate component
<FeatureGate feature="CUSTOM_DOMAINS" featureName="Custom Domains">
  <CustomDomainConfig />
</FeatureGate>
```

## Task 3: Add a Frontend Page

```typescript
// client/app/dashboard/widgets/page.tsx
"use client"

import { useRouter } from "next/navigation"
import { useApi } from "@/lib/hooks/use-api"
import { platformApi } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import { useToast } from "@/hooks/use-toast"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Badge } from "@/components/ui/badge"
import { Plus } from "lucide-react"

export default function WidgetsPage() {
  const router = useRouter()
  const { getToken } = useAuth()
  const { toast } = useToast()

  const { data: widgets, loading, refetch } = useApi(
    async () => {
      const token = await getToken()
      if (!token) return null
      const res = await platformApi.widget.listWidgets({
        headers: { authorization: `Bearer ${token}` }
      })
      return res.code === 200 ? res.response : null
    },
    { deps: [] }
  )

  if (loading) return <div>Loading...</div>

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Widgets</h1>
        <Button onClick={() => router.push('/dashboard/widgets/new')}>
          <Plus className="h-4 w-4 mr-2" />
          New Widget
        </Button>
      </div>

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {widgets?.map((w) => (
                <TableRow
                  key={w.id}
                  className="cursor-pointer"
                  onClick={() => router.push(`/dashboard/widgets/${w.id}`)}
                >
                  <TableCell className="font-medium">{w.name}</TableCell>
                  <TableCell>
                    <Badge variant={w.status === 'active' ? 'default' : 'secondary'}>
                      {w.status}
                    </Badge>
                  </TableCell>
                  <TableCell>{new Date(w.createdAt).toLocaleDateString()}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}
```

## Task 4: Debug Common Issues

### MikroORM "Entity not found"

```typescript
// Use findOne (returns null) instead of findOneOrFail (throws)
const entity = await em.findOne(Entity, { id });
if (!entity) {
  res.status(404).send("Not found");
  return;
}
```

### "Cannot flush changes without active transaction"

```typescript
// Make sure you call em.flush() after mutations
em.persist(entity);
await em.flush(); // DON'T FORGET THIS
```

### "Property X requires initialization"

```typescript
// Add `!` assertion to required properties
@Property()
name!: string;  // use ! for required props
```

### Frontend "Cannot read properties of null"

```typescript
// Always check token and response code
const token = await getToken()
if (!token) return null  // handle unauthenticated

const res = await platformApi.service.get(...)
if (res.code !== 200) return null  // handle error
```

## Task 5: Run Migrations

```bash
# Create a new migration
pnpm migrate:create

# Run all pending migrations
pnpm migrate:up

# Rollback last migration
pnpm migrate:down

# Check migration status
pnpm migrate:list
```

Never run raw migration CLI commands — always use `pnpm` scripts from the module's `package.json`.
