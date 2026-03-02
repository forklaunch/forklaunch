---
title: Changing Projects
category: Guides
description: Learn how to modify and update existing ForkLaunch projects safely and efficiently.
---

## Overview

ForkLaunch provides powerful commands to modify existing projects, allowing you to update configurations, change technologies, and adapt your application as requirements evolve. This guide covers the core concepts and best practices for safely changing ForkLaunch projects.

The `forklaunch change` command family allows you to modify:

- **Applications**: Runtime, frameworks, tools, and configuration
- **Services**: Database types, infrastructure, and service configuration
- **Workers**: Worker types, queue systems, and processing logic
- **Routers**: Router configuration and naming
- **Libraries**: Library metadata and configuration

All change operations are designed to be safe, reversible, and maintain project integrity.

## How Changes Affect Artifacts

When you change a project, ForkLaunch automatically updates **application artifacts** - configuration files that manage your entire application:

| Artifact | What Gets Updated |
|----------|-------------------|
| **Manifest** (`.forklaunch/manifest.toml`) | Project metadata, configuration, and dependencies |
| **Docker Compose** (`docker-compose.yaml`) | Service definitions, ports, and infrastructure |
| **Workspace Config** (`pnpm-workspace.yaml` or `package.json`) | Package references and scripts |
| **Universal SDK** (`modules/universal-sdk/`) | API client generation (for services) |
| **TypeScript Config** (`modules/tsconfig.json`) | Project references |

**Example**: Changing a service's database from SQLite to PostgreSQL updates:
- The service's `mikro-orm.config.ts` file
- Docker Compose to add PostgreSQL container
- Package dependencies in workspace config
- Migration scripts in package.json

After making changes, you may need to:
1. Install updated dependencies: `pnpm install` (or `bun install`)
2. Run migrations if database changed: `pnpm migrate:init` (or `bun migrate:init`)
3. Rebuild the project: `pnpm build` (or `bun run build`)

## Core Principles

### Safety First

- Always commit your changes to version control before making modifications
- Use `--dry-run` to preview changes before applying them
- Test thoroughly after each change operation

### Incremental Changes

- Make one change at a time to minimize risk
- Test each change before proceeding to the next
- Keep related changes grouped in separate commits

### Consistency

- Maintain consistent configuration across all components
- Use `forklaunch depcheck` to verify dependency alignment
- Update documentation and environment files as needed

## Quick Start

### Preview Changes

Before making any changes, preview what will happen:

```bash
# Preview application changes
forklaunch change application --runtime bun --dry-run

# Preview service changes
forklaunch change service --database postgresql --dry-run
```

### Make Changes Safely

Always follow this workflow:

```bash
# 1. Commit current state
git add .
git commit -m "Before changing runtime to Bun"

# 2. Make the change
forklaunch change application --runtime bun

# 3. Install updated dependencies
```

<CodeTabs type="terminal">
  <Tab title="pnpm">

```bash
pnpm install
```

  </Tab>
  <Tab title="bun">

```bash
bun install
```

  </Tab>
</CodeTabs>

```bash
# 4. Test the changes
```

<CodeTabs type="terminal">
  <Tab title="pnpm">

```bash
pnpm test
pnpm run dev
```

  </Tab>
  <Tab title="bun">

```bash
bun test
bun run dev
```

  </Tab>
</CodeTabs>

```bash
# 5. Commit the changes
git add .
git commit -m "Changed runtime to Bun"
```

## Common Change Scenarios

### Technology Upgrades

Update your technology stack as projects evolve:

```bash
# Modern JavaScript stack
forklaunch change application \
  --runtime bun \
  --formatter biome \
  --linter oxlint

# High-performance HTTP framework
forklaunch change application --http-framework hyper-express

# Modern validation library
forklaunch change application --validator typebox
```

### Database Migration

Change database systems safely:

```bash
# Upgrade from SQLite to PostgreSQL
forklaunch change service --database postgresql

# Add Redis for caching
forklaunch change service --infrastructure redis
```

### Development Tools

Update development and build tools:

```bash
# Switch to modern tooling
forklaunch change application \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest
```

## Component-Specific Guides

### Applications

Learn how to modify application-level configuration, runtimes, and frameworks.

👉 **[Changing Applications](./changing-projects/applications.md)**

- Runtime changes (Node.js ↔ Bun)
- HTTP framework updates (Express ↔ Hyper-Express)
- Development tool changes (Prettier ↔ Biome, ESLint ↔ oxlint)
- Testing framework updates (Jest ↔ Vitest)
- Validation library changes (Zod ↔ TypeBox)

### Services

Modify service configuration, databases, and infrastructure components.

👉 **[Changing Services](./changing-projects/services.md)**

- Database type changes (SQLite → PostgreSQL, MySQL, etc.)
- Infrastructure additions (Redis, message queues)
- Service name and description updates
- Configuration and environment changes

### Workers

Update worker types, queue systems, and processing configurations.

👉 **[Changing Workers](./changing-projects/workers.md)**

- Worker type changes (Database → Redis → Kafka → BullMQ)
- Queue system migrations
- Database configuration for database workers
- Processing logic updates

### Routers

Modify router configurations and naming.

👉 **[Changing Routers](./changing-projects/routers.md)**

- Router renaming and reorganization
- Configuration updates
- Route structure modifications

### Libraries

Update library configuration and metadata.

👉 **[Changing Libraries](./changing-projects/libraries.md)**

- Library name and description updates
- Configuration changes
- Export structure modifications

## Best Practices

### Before Making Changes

1. **Version Control**: Commit all current changes
2. **Dependencies**: Run `forklaunch depcheck` to check current state
3. **Testing**: Ensure all tests pass
4. **Documentation**: Review current configuration

### During Changes

1. **Incremental**: Make one change at a time
2. **Preview**: Use `--dry-run` for complex changes
3. **Monitoring**: Watch for error messages and warnings
4. **Validation**: Check file modifications and dependency updates

### After Changes

1. **Dependencies**: Install updated packages
   <CodeTabs type="terminal">
   <Tab title="pnpm">

```bash
pnpm install
```

  </Tab>
  <Tab title="bun">

```bash
bun install
```

  </Tab>
</CodeTabs>

2. **Testing**: Run full test suite
   <CodeTabs type="terminal">
   <Tab title="pnpm">

```bash
pnpm test
```

  </Tab>
  <Tab title="bun">

```bash
bun test
```

  </Tab>
</CodeTabs>

3. **Development**: Test in development environment
   <CodeTabs type="terminal">
   <Tab title="pnpm">

```bash
pnpm run dev
```

  </Tab>
  <Tab title="bun">

```bash
bun run dev
```

  </Tab>
</CodeTabs>

4. **Documentation**: Update README and environment files
5. **Commit**: Commit changes with descriptive messages

## Advanced Scenarios

### Batch Changes

Make multiple related changes efficiently:

```bash
# Complete stack modernization
forklaunch change application \
  --runtime bun \
  --http-framework hyper-express \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest \
  --validator typebox
```

### Environment-Specific Changes

Handle different requirements across environments:

```bash
# Development optimized
forklaunch change application --formatter prettier --linter eslint

# Production optimized
forklaunch change application --formatter biome --linter oxlint
```

### Migration Scripts

Create reusable scripts for common change patterns:

```bash
#!/bin/bash
# scripts/upgrade-stack.sh
echo "Upgrading to modern development stack..."

forklaunch change application --runtime bun
forklaunch change application --formatter biome
forklaunch change application --linter oxlint

```

<CodeTabs type="terminal">
  <Tab title="pnpm">

```bash
pnpm install
pnpm test
```

  </Tab>
  <Tab title="bun">

```bash
bun install
bun test
```

  </Tab>
</CodeTabs>

```bash
echo "Stack upgrade complete!"
```

## Troubleshooting

### Common Issues

**Dependency Conflicts**
<CodeTabs type="terminal">
<Tab title="pnpm">

```bash
rm -rf node_modules pnpm-lock.yaml
pnpm install
```

  </Tab>
  <Tab title="bun">

```bash
rm -rf node_modules bun.lockb bun.lock
bun install
```

  </Tab>
</CodeTabs>

**Configuration Errors**

```bash
forklaunch depcheck
```

<CodeTabs type="terminal">
  <Tab title="pnpm">

```bash
pnpm run lint
```

  </Tab>
  <Tab title="bun">

```bash
bun run lint
```

  </Tab>
</CodeTabs>

**Runtime Errors**
<CodeTabs type="terminal">
<Tab title="pnpm">

```bash
pnpm run dev
# Check logs for specific error messages
```

  </Tab>
  <Tab title="bun">

```bash
bun run dev
# Check logs for specific error messages
```

  </Tab>
</CodeTabs>

## Migration Considerations

### Major Version Changes

When upgrading ForkLaunch versions:

1. Review [Migration Guide](./migration.md) for breaking changes
2. Update CLI to latest version
3. Run `forklaunch depcheck` to identify issues
4. Apply necessary configuration changes
5. Test thoroughly in development environment

### Project Structure Changes

Some changes may affect project structure:

- File relocations and renames
- New configuration files
- Updated dependency requirements
- Modified build processes

Always review git diff after changes to understand the full impact.

## Related Documentation

- **[Adding Projects](./adding-projects.md)** - Creating new components
- **[CLI Reference](./cli.md)** - Complete command reference
- **[Best Practices](./best-practices.md)** - Development best practices
- **[Troubleshooting](./troubleshooting.md)** - Common issues and solutions
- **[Deployment](./deployment.md)** - Production deployment strategies
