---
name: tanstack
description: "TanStack Start: full-stack React framework. Routing, server functions, SSR, data loading. Use for ForkLaunch frontends."
user-invokable: true
---

# TanStack Start Integration with ForkLaunch

TanStack Start is a full-stack React framework powered by TanStack Router. It provides SSR, streaming, server functions, and file-based routing. Use it as the frontend for ForkLaunch backends.

Docs: tanstack.com/start/latest

## Project Setup

### Install
```bash
npm i @tanstack/react-start @tanstack/react-router react react-dom
npm i -D vite @vitejs/plugin-react typescript @types/react @types/react-dom @types/node
```

### Vite Config
```ts
// vite.config.ts
import { defineConfig } from 'vite'
import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import viteReact from '@vitejs/plugin-react'

export default defineConfig({
  server: { port: 3000 },
  plugins: [tanstackStart(), viteReact()],
})
```

### File Structure
```
src/
├── routes/
│   ├── __root.tsx          # Root layout (html, head, body)
│   ├── index.tsx           # / route
│   ├── about.tsx           # /about route
│   └── restaurants/
│       ├── index.tsx        # /restaurants
│       └── $id.tsx          # /restaurants/:id
├── router.tsx              # Router creation
└── routeTree.gen.ts        # Auto-generated route tree
```

### Router
```tsx
// src/router.tsx
import { createRouter } from '@tanstack/react-router'
import { routeTree } from './routeTree.gen'

export function getRouter() {
  return createRouter({ routeTree, scrollRestoration: true })
}
```

### Root Route
```tsx
// src/routes/__root.tsx
import type { ReactNode } from 'react'
import { Outlet, createRootRoute, HeadContent, Scripts } from '@tanstack/react-router'

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'My App' },
    ],
  }),
  component: () => (
    <html>
      <head><HeadContent /></head>
      <body><Outlet /><Scripts /></body>
    </html>
  ),
})
```

## Core Patterns

### File-Based Routes

Files in `src/routes/` auto-map to URL paths:
- `index.tsx` → `/`
- `about.tsx` → `/about`
- `restaurants/index.tsx` → `/restaurants`
- `restaurants/$id.tsx` → `/restaurants/:id`
- `_layout.tsx` → Layout wrapper (no URL segment)

### Page with Data Loading
```tsx
// src/routes/restaurants/index.tsx
import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'

const getRestaurants = createServerFn({ method: 'GET' })
  .handler(async () => {
    const res = await fetch(`${process.env.API_URL}/restaurants`)
    return res.json()
  })

export const Route = createFileRoute('/restaurants/')({
  component: RestaurantsPage,
  loader: async () => await getRestaurants(),
})

function RestaurantsPage() {
  const restaurants = Route.useLoaderData()
  return (
    <div>
      <h1>Restaurants</h1>
      {restaurants.map(r => <div key={r.id}>{r.name}</div>)}
    </div>
  )
}
```

### Server Functions (RPC)
```tsx
import { createServerFn } from '@tanstack/react-start'

// GET: fetch data
const getRestaurant = createServerFn({ method: 'GET' })
  .inputValidator((id: string) => id)
  .handler(async ({ data: id }) => {
    const res = await fetch(`${process.env.API_URL}/restaurants/${id}`)
    return res.json()
  })

// POST: mutate data
const createOrder = createServerFn({ method: 'POST' })
  .inputValidator((order: { restaurantId: string; items: string[] }) => order)
  .handler(async ({ data }) => {
    const res = await fetch(`${process.env.API_URL}/orders`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    })
    return res.json()
  })
```

### Dynamic Route with Params
```tsx
// src/routes/restaurants/$id.tsx
import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/restaurants/$id')({
  component: RestaurantDetail,
  loader: async ({ params }) => await getRestaurant({ data: params.id }),
})

function RestaurantDetail() {
  const restaurant = Route.useLoaderData()
  return <h1>{restaurant.name}</h1>
}
```

### Search Params (Query Strings)
```tsx
import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'

const searchSchema = z.object({
  query: z.string().optional(),
  page: z.number().optional().default(1),
})

export const Route = createFileRoute('/search')({
  validateSearch: searchSchema,
  component: SearchPage,
  loader: async ({ search }) => await searchFn({ data: search }),
})

function SearchPage() {
  const { query, page } = Route.useSearch()
  // ...
}
```

## Connecting to ForkLaunch Backend

### Using the Generated SDK
```tsx
// src/lib/api.ts
import { createClient } from '@<app-name>/client-sdk'

export const api = createClient({
  baseUrl: process.env.API_URL || 'http://localhost:8000',
})
```

### Server Function with ForkLaunch SDK
```tsx
import { createServerFn } from '@tanstack/react-start'
import { api } from '../lib/api'

const getRestaurants = createServerFn({ method: 'GET' })
  .handler(async () => {
    const result = await api.restaurants.list({
      headers: { /* auth headers */ },
    })
    if (result.code !== 200) throw new Error(`API error: ${result.code}`)
    return result.response
  })
```

### Auth Integration

Share JWT between TanStack Start and ForkLaunch:

```tsx
// src/lib/auth.ts
import { createServerFn } from '@tanstack/react-start'

export const getSession = createServerFn({ method: 'GET' })
  .handler(async ({ request }) => {
    const cookie = request.headers.get('cookie')
    // Validate JWT against ForkLaunch's JWKS endpoint
    // Return session data
  })
```

### Middleware for Auth
```tsx
// src/routes/__root.tsx
import { createRootRouteWithContext } from '@tanstack/react-router'

interface RouterContext {
  session: Session | null
}

export const Route = createRootRouteWithContext<RouterContext>()({
  beforeLoad: async () => {
    const session = await getSession()
    return { session }
  },
  component: RootComponent,
})
```

## TanStack Query (Data Fetching in Components)

For client-side data fetching beyond route loaders:

```tsx
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'

function RestaurantMenu({ restaurantId }: { restaurantId: string }) {
  const { getToken } = useAuth()

  const { data: menu } = useQuery({
    queryKey: ['menu', restaurantId],
    queryFn: async () => {
      const token = await getToken()
      if (!token) throw new Error('Not authenticated')
      const result = await api.restaurants.getMenu({
        params: { id: restaurantId },
        headers: { Authorization: `Bearer ${token}` },
      })
      if (result.code !== 200) throw new Error(`API error: ${result.code}`)
      return result.response
    },
  })

  const queryClient = useQueryClient()
  const addItem = useMutation({
    mutationFn: async (item) => {
      const token = await getToken()
      if (!token) throw new Error('Not authenticated')
      const result = await api.orders.addItem({
        body: item,
        headers: { Authorization: `Bearer ${token}` },
      })
      if (result.code !== 201) throw new Error(`API error: ${result.code}`)
      return result.response
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['cart'] }),
  })

  return (
    <div>
      {menu?.map(item => (
        <button key={item.id} onClick={() => addItem.mutate(item)}>
          {item.name} - ${item.price}
        </button>
      ))}
    </div>
  )
}
```

## TanStack Form (Forms)

```tsx
import { useForm } from '@tanstack/react-form'

function CreateRestaurantForm() {
  const { getToken } = useAuth()
  const [error, setError] = useState<string | null>(null)

  const form = useForm({
    defaultValues: { name: '', address: '', cuisineType: '' },
    onSubmit: async ({ value }) => {
      const token = await getToken()
      if (!token) { setError('Not authenticated'); return }
      const result = await api.restaurants.create({
        body: value,
        headers: { Authorization: `Bearer ${token}` },
      })
      if (result.code !== 201) { setError(`Failed: ${result.code}`); return }
      setError(null)
      // success — navigate or show toast
    },
  })

  return (
    <form onSubmit={(e) => { e.preventDefault(); form.handleSubmit() }}>
      {error && <p className="text-red-500">{error}</p>}
      <form.Field name="name" children={(field) => (
        <input value={field.state.value} onChange={(e) => field.handleChange(e.target.value)} />
      )} />
      <button type="submit">Create</button>
    </form>
  )
}
```

## TanStack Table (Data Tables)

```tsx
import { useReactTable, getCoreRowModel, flexRender } from '@tanstack/react-table'

function OrdersTable({ orders }) {
  const columns = [
    { accessorKey: 'id', header: 'Order ID' },
    { accessorKey: 'status', header: 'Status' },
    { accessorKey: 'total', header: 'Total', cell: ({ getValue }) => `$${getValue()}` },
  ]

  const table = useReactTable({ data: orders, columns, getCoreRowModel: getCoreRowModel() })

  return (
    <table>
      <thead>
        {table.getHeaderGroups().map(hg => (
          <tr key={hg.id}>
            {hg.headers.map(h => <th key={h.id}>{flexRender(h.column.columnDef.header, h.getContext())}</th>)}
          </tr>
        ))}
      </thead>
      <tbody>
        {table.getRowModel().rows.map(row => (
          <tr key={row.id}>
            {row.getVisibleCells().map(cell => <td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>)}
          </tr>
        ))}
      </tbody>
    </table>
  )
}
```

## Deployment

TanStack Start deploys to any Vite-compatible host:
- **Vercel**: Zero-config, recommended for ForkLaunch projects
- **Cloudflare Workers**: Edge deployment
- **Node.js**: Standard server deployment
- **Netlify**: Static + serverless

For ForkLaunch projects, deploy the TanStack Start frontend to Vercel and the ForkLaunch backend to AWS via Pulumi. Configure `API_URL` in environment variables.
