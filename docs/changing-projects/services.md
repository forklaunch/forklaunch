---
title: Changing Services
category: Changing Projects
description: Complete guide for modifying ForkLaunch service configuration, databases, and infrastructure.
---

## Overview

The `forklaunch change service` command allows you to modify existing service configuration including database types, infrastructure components, and service metadata. This guide covers all available options and migration strategies.

<CodeTabs type="instantiate">
  <Tab title="Basic">

  ```bash
  forklaunch change service
  ```

  </Tab>
  <Tab title="With Options">

  ```bash
  forklaunch change service --path ./my-service --database postgresql
  ```

  </Tab>
</CodeTabs>

## Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--path` | `-p` | The service path | Path to service directory |
| `--name` | `-N` | The name of the service | Any valid service name |
| `--database` | `-d` | The database to use | See database options below |
| `--description` | `-D` | The description of the service | Any string |
| `--infrastructure` | `-i` | The infrastructure to use (can specify multiple) | `redis`, `s3` |
| `--dryrun` | `-n` | Dry run the command | Flag (no value) |
| `--confirm` | `-c` | Flag to confirm any prompts | Flag (no value) |

## Database Options

Available database types:

- `postgresql` - PostgreSQL database
- `mysql` - MySQL database
- `mariadb` - MariaDB database
- `mssql` - Microsoft SQL Server
- `mongodb` - MongoDB database
- `libsql` - LibSQL (SQLite-compatible)
- `sqlite` - SQLite database
- `better-sqlite` - Better SQLite3

## Infrastructure Changes

### Adding Infrastructure

Add infrastructure, e.g. Redis for caching and session management:

```bash
forklaunch change service --path ./user-service --infrastructure redis
```

### Removing Infrastructure

Remove Redis if no longer needed:

```bash
forklaunch change service --path ./user-service --infrastructure
```

Note: Pass empty value to remove all infrastructure dependencies.

## Batch Changes

### Complete Service Upgrade

Upgrade database and add infrastructure:

```bash
forklaunch change service \
  --path ./user-service \
  --database postgresql \
  --infrastructure redis \
  --description "High-performance user management service"
```

### Multiple Services

Update multiple services with same configuration:

```bash
# Upgrade all services to PostgreSQL
forklaunch change service --path ./user-service --database postgresql
forklaunch change service --path ./billing-service --database postgresql
forklaunch change service --path ./analytics-service --database postgresql
```

## Migration Strategies

### Development to Production Migration

#### Phase 1: Prepare Development

````bash
# Update to production database
forklaunch change service --path ./user-service --database postgresql

# Add Redis for performance
forklaunch change service --path ./user-service --infrastructure redis

# Test thoroughly
<CodeTabs type="terminal">
  <Tab title="pnpm">

  ```bash
  pnpm test
  pnpm run dev
````

  </Tab>
  <Tab title="bun">

```bash
bun test
bun run dev
```

  </Tab>
</CodeTabs>
```

#### Phase 2: Data Migration

<CodeTabs type="terminal">
  <Tab title="pnpm">

```bash
# Create migration scripts
pnpm run migration:generate -- CreateUserTable
pnpm run migration:run

# Verify data migration
pnpm run migration:show
```

  </Tab>
  <Tab title="bun">

```bash
# Create migration scripts
bun run migration:generate -- CreateUserTable
bun run migration:run

# Verify data migration
bun run migration:show
```

  </Tab>
</CodeTabs>

#### Phase 3: Environment Updates

Update environment variables:

```bash
# .env.production
DB_TYPE=postgresql
DB_HOST=prod-db.example.com
DB_PORT=5432
DB_NAME=myapp_prod
DB_USER=myapp_user
DB_PASSWORD=secure_password

REDIS_HOST=prod-redis.example.com
REDIS_PORT=6379
REDIS_PASSWORD=redis_password
```

### Cross-Database Migration

AUTO-MIGRATION PLANNED

## Common Scenarios

### Scenario 1: Scale-Up for Production

Prepare service for production load:

```bash
# Step 1: Upgrade to PostgreSQL
forklaunch change service --path ./user-service --database postgresql

# Step 2: Add Redis for caching
forklaunch change service --path ./user-service --infrastructure redis

# Step 3: Update service description
forklaunch change service --path ./user-service --description "Production-ready user management service with Redis caching"
```

### Scenario 2: Development Simplification

Simplify for local development:

```bash
# Use SQLite for easier local setup
forklaunch change service --path ./user-service --database sqlite

# Remove Redis dependency
forklaunch change service --path ./user-service --infrastructure
```

### Scenario 3: Microservice Refactoring

Split monolithic service:

```bash
# Create specialized services
forklaunch change service --path ./user-service --name authentication-service \
  --description "User authentication and authorization"

# Add new user profile service separately
forklaunch init service profile-service --path ./my-app/src/modules --database postgresql
```

## Best Practices

### Pre-Change Checklist

1. **Backup data**: Export current database
2. **Version control**: Commit all changes
3. **Test suite**: Ensure all tests pass
4. **Documentation**: Review current configuration

### Change Process

1. **Preview changes**: Use `--dry-run` for complex changes
2. **Incremental updates**: Change one aspect at a time
3. **Test immediately**: Verify changes work
4. **Update environment**: Sync configuration files

### Post-Change Validation

| Step              | pnpm                                | bun                                 |
| :---------------- | :---------------------------------- | :---------------------------------- |
| **Dependencies**  | `pnpm install`                      | `bun install`                       |
| **Database**      | Test connections and migrations     | Test connections and migrations     |
| **Integration**   | Run full test suite                 | Run full test suite                 |
| **Development**   | Start and test locally              | Start and test locally              |
| **Documentation** | Update README and environment files | Update README and environment files |

## Related Documentation

- **[Changing Applications](./applications)** - Application-level changes
- **[Adding Services](../adding-projects/services)** - Creating new services
- **[Database Configuration](../framework/database)** - Database setup and configuration
- **[Infrastructure Integration](../framework/infrastructure)** - Infrastructure components
