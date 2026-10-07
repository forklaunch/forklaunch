---
title: Welcome to ForkLaunch
category: Guides
description: An introduction to ForkLaunch: what it is, how it works, and where to start.
---

## Welcome

ForkLaunch is a developer platform for building production-ready TypeScript applications. Its your stepping stone from prototype to production. It manages your application artifacts to minimize your headache so that production systems don't break with code changes. 

Whether you're starting from scratch, wiring up AI-assisted workflows, or migrating an existing codebase, ForkLaunch gives you a structured foundation that grows with your system with minimal manual configuration.

ForkLaunch is built for teams who want to move fast without accumulating the kind of structural debt that makes systems brittle. Whether you're a solo developer shipping your first production backend or a senior engineer building infrastructure for a growing team, the goal is the same: a system that's safe to change.

## What is ForkLaunch?

ForkLaunch is three things working together:

**A CLI scaffold tool.** Run a single command and get a fully wired service with routes, controllers, entities, mappers, a database configuration, and a docker-compose entry, ready to run locally and deploy to production.

**A type-safe framework layer.** Define your API contracts once using TypeScript validator schemas. From that single definition ForkLaunch generates TypeScript types for compile-time safety, runtime request validation, OpenAPI documentation, and typed client SDKs. When a contract changes, every service that depends on it changes or fails at compile time, not in production.

**A deployment and observability platform.** Deploy to your own AWS account or ForkLaunch's managed cloud in two commands. Distributed tracing, structured logging, and dependency graphs are wired in through OpenTelemetry with zero configuration. Export your entire infrastructure as Pulumi TypeScript at any time.

### Key principles

- **AI implements, ForkLaunch enforces.** Use Claude Code, Cursor, or any AI assistant to generate implementation. ForkLaunch enforces architecture at compile time so AI-generated code can't silently break your system.
- **Break builds, not production.** Refactoring, API changes, and architectural evolution, such as converting services to workers, extracting libraries, and splitting monoliths, all surface as compile errors before they reach a deployed environment.
- **No magic, no lock-in.** Every generated file is standard TypeScript. Infrastructure exports to standard Pulumi. SDKs are TypeScript imports, not proprietary runtimes. Walk away at any time.
- **Sensible defaults, not enforced structure.** Generated projects follow clear separation of concerns. The patterns are suggestions, not constraints.

## Guides

Learn how to get started with ForkLaunch by checking out these guides.

| Guide | Description |
| :---- | :---------- |
| [Getting Started](/docs/getting-started.md) | Install the CLI and get your environment ready |
| [Creating an Application](/docs/creating-an-application.md) | Initialize your first ForkLaunch application |
| [Adding Projects](/docs/adding-projects.md) | Add services, workers, libraries, and routers |
| [Changing Projects](/docs/changing-projects.md) | Modify existing project configuration |
| [Deleting Projects](/docs/deleting-projects.md) | Remove services and clean up artifacts |
| [Local Development](/docs/local-development.md) | Run your full stack locally with docker-compose |
| [Preconfigured Services](/docs/preconfigured-services.md) | Use ready-made service blueprints |
| [Customization](/docs/customization.md) | Adapt generated code to your conventions |

## Advanced Guides

Go deeper on specific capabilities once your project is running.

| Guide | Description |
| :---- | :---------- |
| [Contract-First Development](/docs/guides/contract-first-development.md) | Design APIs from schema outward |
| [Dependency Management](/docs/guides/dependency-management.md) | Manage cross-service dependencies safely |
| [AsyncAPI Generation](/docs/guides/asyncapi.md) | Generate AsyncAPI specs for event-driven services |
| [Pulumi Export](/docs/guides/pulumi-export.md) | Export infrastructure as standard Pulumi TypeScript |
| [Cache](/docs/guides/cache.md) | Use TTL-based caching with Redis |
| [Object Store](/docs/guides/objectstore.md) | Store and stream large files with S3 |
| [WebSockets](/docs/guides/websockets.md) | Build real-time APIs with type-safe WebSocket events |
| [Testing](/docs/guides/testing.md) | Unit and integration testing patterns |
| [CLI Commands](/docs/guides/cli-commands.md) | Common CLI workflows and task sequences |

## Knowledge Base

Deeper reading on how ForkLaunch works under the hood.

| Page | Description |
| :---- | :---------- |
| [Features](/docs/learn/features.md) | Complete overview of ForkLaunch capabilities |
| [Project Basics](/docs/learn/project-basics.md) | Understand applications, services, workers, and libraries |
| [Artifacts](/docs/learn/artifacts.md) | What ForkLaunch generates and why |
| [Architecture](/docs/learn/architecture.md) | System design and internal structure |

## Reference

Full command and framework documentation.

| Reference | Description |
| :-------- | :---------- |
| [CLI Reference](/docs/cli.md) | All CLI commands and options |
| [Framework Reference](/docs/framework.md) | HTTP, validation, telemetry, authorization, and more |
