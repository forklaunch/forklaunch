---
name: frontend-patterns
description: "Frontend: pages, SDK client, useApi/useMutation hooks, auth, feature gating, forms, tables."
user-invokable: true
---

# ForkLaunch Frontend Patterns

## When to Use This Skill

Use when the user asks to:

- Build or modify Next.js pages in the dashboard
- Add API calls to frontend components
- Implement feature gating in the UI
- Create forms, dialogs, or data tables
- Work with authentication or authorization on the frontend
- Add data fetching, polling, or mutations

## Running

```bash
cd client && pnpm install    # Install frontend deps
cd client && pnpm dev        # Next.js dev server (default: localhost:3000)
cd client && pnpm tsgo --noEmit  # Type check (pre-existing errors in resource explorers — ignore those)
cd client && pnpm build      # Production build
```

**Prerequisite:** Backend services must be running (`docker compose up -d` + `pnpm dev` from repo root or individual modules).

## Tech Stack

- **Framework:** Next.js 16 (app router, React 19)
- **UI:** Shadcn/Radix UI components + Tailwind CSS
- **Forms:** react-hook-form + Shadcn Form components
- **State:** Component-level `useState`, minimal Jotai atoms
- **API:** Auto-generated typed SDK clients (NOT raw fetch/axios)
- **Auth:** AuthContext with JWT tokens from Better Auth

## Page Component Pattern

Every dashboard page follows this structure:

```typescript
"use client"

import { useParams, useRouter } from "next/navigation"
import { useEffect, useState } from "react"
import { useApi, useMutation } from "@/lib/hooks/use-api"
import { platformApi } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import { useToast } from "@/hooks/use-toast"
import { useFeatureAccess } from "@/hooks/use-feature-access"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Badge } from "@/components/ui/badge"

export default function ServiceDetailPage() {
  const params = useParams()
  const router = useRouter()
  const { getToken } = useAuth()
  const { toast } = useToast()
  const { checkFeature } = useFeatureAccess()

  // --- Data Fetching ---
  const { data: service, loading, error, refetch } = useApi(
    async () => {
      const token = await getToken()
      if (!token) return null
      const response = await platformApi.service.getService({
        params: { id: params.id as string },
        headers: { authorization: `Bearer ${token}` }
      })
      return response.code === 200 ? response.response : null
    },
    { deps: [params.id], pollInterval: 5000 }
  )

  // --- Mutations ---
  const { mutate: updateService, loading: updating } = useMutation()

  const handleUpdate = async (data: Record<string, unknown>) => {
    await updateService(async () => {
      const token = await getToken()
      const response = await platformApi.service.updateService({
        params: { id: params.id as string },
        body: data,
        headers: { authorization: `Bearer ${token}` }
      })
      if (response.code === 200) {
        toast({ description: "Service updated successfully" })
        await refetch()
      } else {
        toast({ variant: "destructive", description: "Failed to update" })
      }
    })
  }

  // --- Loading / Error States ---
  if (loading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </div>
    )
  }

  if (!service) {
    return <div className="text-center py-8 text-muted-foreground">Service not found</div>
  }

  // --- Render ---
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">{service.name}</h1>
          <p className="text-muted-foreground">{service.description}</p>
        </div>
        <Badge variant={service.status === "running" ? "default" : "secondary"}>
          {service.status}
        </Badge>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Configuration</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Content here */}
        </CardContent>
      </Card>
    </div>
  )
}
```

## API Client Usage

SDK clients are pre-initialized in `client/lib/api.ts`:

```typescript
import {
  platformManagementSdkClient,
  iamSdkClient,
  billingApiSdkClient,
} from "../../src/modules/universal-sdk";

export const platformApi = await platformManagementSdkClient({
  host: process.env.NEXT_PUBLIC_PLATFORM_URL || "http://localhost:8004",
  registryOptions: { path: "api/v1/openapi" },
});

export const iamApi = await iamSdkClient({
  host: process.env.NEXT_PUBLIC_IAM_URL || "http://localhost:8001",
  registryOptions: { path: "api/v1/openapi" },
});

export const billingApi = await billingApiSdkClient({
  host: process.env.NEXT_PUBLIC_BILLING_URL || "http://localhost:8000",
  registryOptions: { path: "api/v1/openapi" },
});
```

**Response shape:** `{ code: number, response: T }`

```typescript
// GET with params
const response = await platformApi.service.getService({
  params: { id: "service-uuid" },
  headers: { authorization: `Bearer ${token}` },
});
if (response.code === 200) {
  const service = response.response; // fully typed from OpenAPI spec
}

// POST with body
const response = await platformApi.service.createService({
  body: { name: "my-service", applicationId: "app-uuid", version: "1.0.0" },
  headers: { authorization: `Bearer ${token}` },
});

// GET with query params
const response = await platformApi.service.listServices({
  query: { applicationId: "app-uuid" },
  headers: { authorization: `Bearer ${token}` },
});

// PATCH with params + body
const response = await platformApi.service.updateService({
  params: { id: "service-uuid" },
  body: { name: "new-name" },
  headers: { authorization: `Bearer ${token}` },
});

// DELETE
const response = await platformApi.service.deleteService({
  params: { id: "service-uuid" },
  headers: { authorization: `Bearer ${token}` },
});
```

**NEVER use raw `fetch()` or `axios`** — always use the typed SDK clients.

## useApi Hook

```typescript
import { useApi } from "@/lib/hooks/use-api";

const {
  data, // T | null — the fetched data
  loading, // boolean — true during initial fetch
  error, // Error | null
  refetch, // () => Promise<void> — manually trigger re-fetch
} = useApi<ServiceType>(
  async () => {
    const token = await getToken();
    if (!token) return null;
    const res = await platformApi.service.getService({
      params: { id },
      headers: { authorization: `Bearer ${token}` },
    });
    return res.code === 200 ? res.response : null;
  },
  {
    deps: [id], // re-fetch when these change (like useEffect deps)
    pollInterval: 5000, // optional: poll every 5 seconds
    skip: !id, // optional: skip fetch if condition is false
    initialData: null, // optional: initial data before first fetch
  },
);
```

## useMutation Hook

```typescript
import { useMutation } from "@/lib/hooks/use-api";

const {
  mutate, // (fn: (vars) => Promise<T>, vars?) => Promise<T | null>
  loading, // boolean
  error, // Error | null
  data, // T | null — last successful result
} = useMutation<ResponseType>();

// Usage
const handleSave = async () => {
  await mutate(async () => {
    const token = await getToken();
    const res = await platformApi.service.updateService({
      params: { id },
      body: formData,
      headers: { authorization: `Bearer ${token}` },
    });
    if (res.code === 200) {
      toast({ description: "Saved!" });
      await refetch();
    } else {
      toast({ variant: "destructive", description: "Failed" });
    }
  });
};
```

## Auth Context

```typescript
import { useAuth } from "@/contexts/auth-context";

const {
  user, // { id, name, email, roles, permissions, organizationId } | null
  isAuthenticated, // boolean
  isLoading, // boolean
  getToken, // () => Promise<string | null>
  hasRole, // (role: string | string[]) => boolean
  hasPermission, // (perm: string | string[]) => boolean
  isAdmin, // boolean
  isEditor, // boolean
  isViewer, // boolean
  signOut, // () => Promise<void>
  refreshUser, // () => Promise<void>
} = useAuth();

// ALWAYS get token before API calls
const token = await getToken();
if (!token) return; // not authenticated
```

## Feature Gating

### Hook: useFeatureAccess

```typescript
import { useFeatureAccess } from "@/hooks/use-feature-access";

const {
  features, // Record<string, boolean>
  checkFeature, // (flag: FeatureFlag) => { hasAccess, showUpgradeModal }
  checkResourceLimit, // (type, currentCount) => { withinLimit, showUpgradeModal }
  limits, // { maxServices, maxEnvironments, ... }
  subscription, // { planName?, planId?, status? }
  isUpgradeModalOpen,
  showUpgradeModal,
  closeUpgradeModal,
} = useFeatureAccess();

// Check feature access
const { hasAccess } = checkFeature("CUSTOM_DOMAINS");
if (!hasAccess) {
  showUpgradeModal();
  return;
}

// Check resource limits
const { withinLimit } = checkResourceLimit("services", currentServiceCount);
if (!withinLimit) {
  showUpgradeModal();
  return;
}
```

### Component: FeatureGate

```typescript
import { FeatureGate } from "@/components/billing/feature-gate"

// Hides children if feature not available
<FeatureGate
  feature="CUSTOM_DOMAINS"
  featureName="Custom Domains"
  featureDescription="Configure custom domains for your services"
>
  <CustomDomainSettings />
</FeatureGate>

// Shows disabled state instead of hiding
<FeatureGate
  feature="AUTO_SCALING"
  featureName="Auto Scaling"
  disableInsteadOfHide
>
  <AutoScalingConfig />
</FeatureGate>
```

Feature flags: `'CUSTOM_DOMAINS'`, `'AUTO_SCALING'`, `'MULTI_REGION'`, `'PRIVATE_NETWORKING'`, etc.

## Dialog Pattern

```typescript
import {
  Dialog, DialogContent, DialogDescription, DialogHeader,
  DialogTitle, DialogFooter
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"

interface ConfigDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSave: (data: ConfigData) => Promise<void>
  initialData?: ConfigData
}

export function ConfigDialog({ open, onOpenChange, onSave, initialData }: ConfigDialogProps) {
  const [saving, setSaving] = useState(false)
  const [formData, setFormData] = useState(initialData)

  const handleSave = async () => {
    if (!formData) return
    setSaving(true)
    try {
      await onSave(formData)
      onOpenChange(false)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>Configure Instance</DialogTitle>
          <DialogDescription>
            Changes will trigger a new deployment.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-4">
          {/* Form fields using Shadcn components */}
          <div className="space-y-2">
            <Label>Instance Type</Label>
            <Select
              value={formData?.instanceType}
              onValueChange={(v) => setFormData({ ...formData, instanceType: v })}
            >
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="small">Small</SelectItem>
                <SelectItem value="medium">Medium</SelectItem>
                <SelectItem value="large">Large</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button onClick={handleSave} disabled={saving}>
            {saving ? "Saving..." : "Save Changes"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

## Form Pattern (react-hook-form)

```typescript
import { useForm } from "react-hook-form"
import {
  Form, FormControl, FormField, FormItem, FormLabel, FormMessage, FormDescription
} from "@/components/ui/form"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Button } from "@/components/ui/button"

interface CreateServiceFormData {
  name: string
  description: string
  version: string
  applicationId: string
}

function CreateServiceForm({
  applications,
  onSubmit
}: {
  applications: { id: string; name: string }[]
  onSubmit: (data: CreateServiceFormData) => Promise<void>
}) {
  const form = useForm<CreateServiceFormData>({
    defaultValues: { name: "", description: "", version: "1.0.0", applicationId: "" }
  })

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
        <FormField
          control={form.control}
          name="name"
          rules={{ required: "Name is required" }}
          render={({ field }) => (
            <FormItem>
              <FormLabel>Service Name</FormLabel>
              <FormControl>
                <Input placeholder="my-service" {...field} />
              </FormControl>
              <FormDescription>Lowercase, hyphens allowed</FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="description"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Description</FormLabel>
              <FormControl>
                <Textarea placeholder="What does this service do?" {...field} />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="applicationId"
          rules={{ required: "Application is required" }}
          render={({ field }) => (
            <FormItem>
              <FormLabel>Application</FormLabel>
              <Select onValueChange={field.onChange} value={field.value}>
                <FormControl>
                  <SelectTrigger>
                    <SelectValue placeholder="Select application" />
                  </SelectTrigger>
                </FormControl>
                <SelectContent>
                  {applications.map((app) => (
                    <SelectItem key={app.id} value={app.id}>{app.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <FormMessage />
            </FormItem>
          )}
        />

        <Button type="submit" disabled={form.formState.isSubmitting}>
          {form.formState.isSubmitting ? "Creating..." : "Create Service"}
        </Button>
      </form>
    </Form>
  )
}
```

## Data Table Pattern

```typescript
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow
} from "@/components/ui/table"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

function ServiceTable({
  services,
  onSelect
}: {
  services: Service[]
  onSelect: (id: string) => void
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Version</TableHead>
          <TableHead>Updated</TableHead>
          <TableHead className="text-right">Actions</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {services.map((service) => (
          <TableRow
            key={service.id}
            className="cursor-pointer"
            onClick={() => onSelect(service.id)}
          >
            <TableCell className="font-medium">{service.name}</TableCell>
            <TableCell>
              <Badge variant={service.status === "running" ? "default" : "secondary"}>
                {service.status}
              </Badge>
            </TableCell>
            <TableCell>{service.version}</TableCell>
            <TableCell>{new Date(service.updatedAt).toLocaleDateString()}</TableCell>
            <TableCell className="text-right">
              <Button variant="ghost" size="sm">View</Button>
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}
```

## Toast Pattern

```typescript
import { useToast } from "@/hooks/use-toast";

const { toast } = useToast();

// Success
toast({ description: "Service created successfully" });

// Error
toast({ variant: "destructive", description: "Failed to create service" });

// With title
toast({
  title: "Deployment Started",
  description: "Your service is being deployed...",
});
```

## Frontend Directory Structure

```
client/
├── app/
│   ├── dashboard/
│   │   ├── services/
│   │   │   ├── [id]/page.tsx     # service detail page
│   │   │   └── page.tsx          # services list page
│   │   ├── workers/
│   │   ├── applications/
│   │   ├── environments/
│   │   ├── deployments/
│   │   └── layout.tsx            # dashboard layout with sidebar
│   ├── login/page.tsx
│   ├── signup/page.tsx
│   ├── layout.tsx                # root layout (AuthProvider, theme)
│   └── page.tsx                  # landing page
├── components/
│   ├── ui/                       # Shadcn/Radix (button, card, dialog, form, etc.)
│   ├── billing/                  # feature-gate, upgrade-modal
│   ├── service-detail/           # service-specific components
│   └── architecture-canvas/     # visualization components
├── contexts/
│   └── auth-context.tsx          # AuthProvider, useAuth hook
├── hooks/
│   ├── use-toast.ts
│   └── use-feature-access.tsx
├── lib/
│   ├── api.ts                    # pre-initialized SDK clients
│   ├── hooks/use-api.ts          # useApi, useMutation
│   └── utils.ts                  # cn() helper, etc.
├── types/
│   └── api.ts                    # re-exported SDK types
└── atoms/                        # Jotai atoms (minimal usage)
```

## Key Conventions

1. **All pages use `"use client"` directive** — Next.js app router with client components
2. **Always get token before API calls** — `const token = await getToken(); if (!token) return null`
3. **SDK response shape is `{ code, response }`** — always check `response.code === 200`
4. **Feature checks use hooks, NOT endpoint probes** — use `useFeatureAccess()` hook
5. **Prefer `useApi` with polling over manual `useEffect`** — use `pollInterval` option
6. **Shadcn components from `@/components/ui/`** — never import from Radix directly
7. **State management is minimal** — `useState` for local state, `useAuth` context for auth, `useFeatureAccess` for features
8. **No raw `fetch()` or `axios`** — always use pre-initialized SDK clients from `@/lib/api`
