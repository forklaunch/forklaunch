## Overview

The cornerstone of `ForkLaunch` is its modular project system. This approach:
- Keeps code organized and maintainable
- Reduces cognitive load
- Provides flexibility for infrastructure decisions

## Understanding Artifacts and Projects

### What Are Artifacts?

**Artifacts** are configuration files that ForkLaunch maintains to keep your application organized. They track which projects exist, how they're configured, and how they integrate with the application.

### The Five Artifacts

| Artifact | Location | Purpose | Updated By |
|----------|----------|---------|------------|
| **Manifest** | `.forklaunch/manifest.toml` | Project registry and metadata | All operations |
| **Docker Compose** | `docker-compose.yaml` | Container service definitions | Services, Workers |
| **Runtime Workspace** | `pnpm-workspace.yaml` or `package.json` | Package manager workspace | All projects |
| **Universal SDK** | `modules/universal-sdk/` | Auto-generated API clients | Services only |
| **TypeScript Config** | `modules/tsconfig.json` | TypeScript project references | Services, Workers, Libraries |

### How Artifacts Change When Initializing Projects with `forklaunch init`

When you initialize a project, ForkLaunch creates or updates the relevant artifacts:

#### Adding a Service
- **Manifest**: Adds service entry with metadata
- **Docker Compose**: Adds service container definition
- **Runtime Workspace**: Adds service to workspace
- **Universal SDK**: Generates API client code
- **TypeScript Config**: Adds project reference

#### Adding a Worker
- **Manifest**: Adds worker entry with metadata
- **Docker Compose**: Adds worker container definition
- **Runtime Workspace**: Adds worker to workspace
- **TypeScript Config**: Adds project reference

#### Adding a Library
- **Manifest**: Adds library entry with metadata
- **Runtime Workspace**: Adds library to workspace
- **TypeScript Config**: Adds project reference

#### Adding Routes to services

### Adding Existing Modules

### How Artifacts Change with Other Operations

#### Changing a Project (`forklaunch change`)
- **Manifest**: Updates project metadata
- **Docker Compose**: Updates container configuration (if applicable)
- **Runtime Workspace**: Updates dependencies
- **Universal SDK**: Regenerates client (services only)


#### Deleting a Project (`forklaunch delete`)
- **Manifest**: Removes project entry
- **Docker Compose**: Removes container definition
- **Runtime Workspace**: Removes from workspace
- **Universal SDK**: Removes client code (services only)
- **TypeScript Config**: Removes project reference


#### Syncing a Project (`forklaunch sync`)
- **Manifest**: Adds project entry (if missing)
- **Docker Compose**: Adds container definition (services/workers only)
- **Runtime Workspace**: Adds to workspace
- **Universal SDK**: Generates client code (services only)
- **TypeScript Config**: Adds project reference

## Available Projects

### Core Projects
- **Modules**: Pre-built services/workers/libraries that help you bootstrap your application
- **Services**: Host HTTP APIs and web applications
- **Workers**: Run asynchronous tasks using event-driven architecture
- **Libraries**: Share common code and business logic

### Project Building Blocks
- **Routers**: Add [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) stack to existing services/workers

### Coming Soon
- **Agents**: Optimized for AI-driven workflows

## Feature Requests

Have an idea for a new project type? We welcome feature requests! Please submit them to our [GitHub Issues](https://github.com/forklaunch/forklaunch-js/issues).