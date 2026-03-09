
<div align="center">
    <img src="./assets/logo.svg" alt="ForkLaunch Logo" width="200">

# ForkLaunch

**Take your weekend hack and make it enterprise-ready by Monday.**

[Get Started](https://forklaunch.com) | [Docs](/docs/getting-started.md) | [Examples](framework/e2e-tests/servers)

</div>

---

## What is ForkLaunch?

ForkLaunch is a backend framework and infrastructure platform that turns application code into production-grade deployments. One-click SOC 2 compliance, full VPC isolation, zero external vendors beyond your cloud provider, on-premable from day one.

Auth, billing, deployments, monitoring — the kind of setup that usually takes a platform team six months, before you've even written your first feature.

We drop in right alongside the code you're already generating with Lovable, Replit, v0, Cursor, and Claude Code.

Whether you're an engineer shipping a SaaS, a founder spinning up an AI agent, or a builder who just wants their thing to work at scale — we meet you where you are.

---

## Why ForkLaunch?

Most Node backends fall into one of two buckets:

| Approach | Problem |
|---|---|
| Hand-built Express | Boilerplate, scaling friction, infra drift |
| Full-stack frameworks | Fast at the start, hard to escape, poor infra support |

ForkLaunch gives you a middle path:

- **Type-safe Express** — Express ergonomics with static typing via Zod or TypeBox
- **CLI-generated architecture** — Services, workers, and libraries created and wired with a single command
- **Infrastructure as code** — Auth, database, cache, storage, queues through ergonomic DI
- **Auto-generated APIs and SDKs** — OpenAPI, AsyncAPI, and typed client SDKs from your route definitions
- **Full observability** — OpenTelemetry metrics, logs, and traces out of the box
- **Incremental adoption** — Drop into any existing Express app, no rewrite required

If you don't like it, you can change it. ForkLaunch is designed to stay out of your way.

---

## Platform

Host agents, deploy services, and go from "it works on my laptop" to closing enterprise deals — without the infrastructure headache.

- **SOC 2 compliance** — one-click audit-ready configuration
- **VPC isolation** — full network isolation per tenant or environment
- **Zero vendor lock-in** — only your cloud provider, nothing else
- **On-prem ready** — deploy anywhere from day one
- **Auth and billing** — built-in identity and payment infrastructure
- **Monitoring and observability** — metrics, logs, and traces bootstrapped automatically
- **Multi-environment deployments** — dev, staging, production with consistent tooling

Already powering realtime video, asynchronous document AI parsing, and cloud management across banking, insurance, consumer, and healthcare.

Self-serve is live at [forklaunch.com](https://forklaunch.com).

---

## Quickstart

### Installation

```bash
npm i -g forklaunch
```

### Create an application

```bash
forklaunch init app <app_name>
```

### Add services, workers, and libraries

```bash
forklaunch init service <service_name>
forklaunch init worker <worker_name>
forklaunch init library <library_name>
```

### Run locally

```bash
# apply initial migrations and schemas
pnpm/bun database:setup

# start all services
pnpm/bun dev
```

Each command adds project files, updates the manifest, registers build and runtime config, and ensures consistent structure across all components.

---

## Framework usage

Drop ForkLaunch into new or existing Express apps:

```ts
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { forklaunchExpress, handlers } from '@forklaunch/express';
import {
  number,
  optional,
  SchemaValidator,
  string,
} from '@forklaunch/validator/zod';

const app = forklaunchExpress(
  SchemaValidator(),
  new OpenTelemetryCollector('my-service')
);

// typed route with auto-generated OpenAPI docs
app.get("/healthz", {
    name: "Health Check",
    summary: "Returns service health",
    responses: { 200: string }
}, (req, res) => {
    res.status(200).send("Ok!");
});

// typed body and response with schema validation
const bodySchema = {
    something: number,
    optionalSomething: optional(string)
};

app.post("/items", {
    name: "Create Item",
    summary: "Creates an item with a hello: world greeting",
    body: bodySchema,
    responses: {
        200: { ...bodySchema, hello: string }
    }
}, (req, res) => {
    res.status(200).json({ ...req.body, hello: 'world' });
});

// register handlers for typed SDK generation
app.registerSdks(..);

app.listen(8000, () => {
    console.log('Server started on 8000');
});
```

---

## Documentation

### Getting Started
| Section | Description |
|---------|-------------|
| [Getting Started](/docs/getting-started.md) | Installation and first run |
| [Creating an Application](/docs/creating-an-application.md) | Core architecture |
| [Local Development](/docs/local-development.md) | Environment and tooling |

### Project Management
| Section | Description |
|---------|-------------|
| [Adding Projects](/docs/adding-projects.md) | Add new services or components |
| [Changing Projects](/docs/changing-projects.md) | Modify existing components |
| [Deleting Projects](/docs/deleting-projects.md) | Remove components safely |

### Framework and CLI
| Section | Description |
|---------|-------------|
| [Framework Guide](/docs/framework.md) | Core patterns |
| [CLI Reference](/docs/cli.md) | Full command reference |
| [Customization](/docs/customization.md) | Override defaults |
| [Preconfigured Services](/docs/preconfigured-services.md) | Built-in modules |

### Community
| Section |
|---------|
| [Contributing](/docs/CONTRIBUTING.md) |
| [Code of Conduct](/docs/CODE_OF_CONDUCT.md) |
| [Security](/docs/SECURITY.md) |

---

## Feedback

Issues, discussions, and contributions are welcome.

ForkLaunch is designed to give you the flexibility of Express with the structure and productivity of a framework — without forcing a rewrite and without hiding the underlying infrastructure.

Let's build something real.
