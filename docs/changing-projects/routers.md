---
title: Changing Routers
category: Changing Projects
description: Complete guide for modifying ForkLaunch router configuration and naming.
---

## Overview

The `forklaunch change router` command allows you to modify existing router configuration including router names and references. This guide covers all available options and common scenarios.

```bash
forklaunch change router [router-name] [options]
```

If no router name is provided, you'll be prompted to select from available routers.

## Available Options

| Option              | Type   | Description     | Default      |
| ------------------- | ------ | --------------- | ------------ |
| `--new-name <name>` | string | New router name | Current name |

## Router Renaming

### Basic Renaming

Rename a router and update all references:

```bash
forklaunch change router user-routes --new-name account-routes
```

**What changes:**

- Router file renamed: `routers/user-routes.ts` → `routers/account-routes.ts`
- Class name updated: `UserRoutes` → `AccountRoutes`
- Import statements updated throughout the project
- Registration calls updated in service files
- Route prefixes updated if they match the router name

## Common Scenarios

### Scenario 1: Refactoring for Clarity

Rename routers to better reflect their purpose:

```bash
# Rename for better semantic meaning
forklaunch change router user-routes --new-name authentication-routes
forklaunch change router admin-routes --new-name management-routes
forklaunch change router api-routes --new-name public-api-routes
```

### Scenario 2: Domain Reorganization

Reorganize routers to match new domain boundaries:

```bash
# Align with new service boundaries
forklaunch change router user-routes --new-name profile-routes
forklaunch change router billing-routes --new-name payment-routes
forklaunch change router analytics-routes --new-name reporting-routes
```

### Scenario 3: API Versioning

Rename routers for API versioning:

```bash
# Add version information to router names
forklaunch change router api-routes --new-name api-v1-routes
forklaunch change router public-routes --new-name public-v2-routes
```

### Router Organization Patterns

#### Functional Organization

```bash
# Group by functionality
forklaunch change router user-routes --new-name authentication-routes
forklaunch change router profile-routes --new-name user-profile-routes
forklaunch change router settings-routes --new-name user-settings-routes
```

#### Domain Organization

```bash
# Group by domain
forklaunch change router payment-routes --new-name billing-payment-routes
forklaunch change router invoice-routes --new-name billing-invoice-routes
forklaunch change router subscription-routes --new-name billing-subscription-routes
```

#### Layer Organization

```bash
# Group by architectural layer
forklaunch change router api-routes --new-name rest-api-routes
forklaunch change router webhook-routes --new-name webhook-handler-routes
forklaunch change router admin-routes --new-name admin-panel-routes
```

## Best Practices

### Naming Conventions

1. **Descriptive Names**: Use clear, descriptive names that indicate the router's purpose

   ```bash
   # Good
   forklaunch change router routes --new-name user-authentication-routes

   # Avoid
   forklaunch change router routes --new-name routes1
   ```

2. **Consistent Patterns**: Follow consistent naming patterns across your project

   ```bash
   # Consistent domain-action pattern
   user-authentication-routes
   user-profile-routes
   billing-payment-routes
   billing-invoice-routes
   ```

3. **Future-Proof Names**: Choose names that won't become outdated quickly

   ```bash
   # Future-proof
   forklaunch change router v1-routes --new-name core-api-routes

   # Less future-proof
   forklaunch change router routes --new-name current-routes
   ```

### Before Renaming

1. **Review Dependencies**: Check what imports the router

   ```bash
   grep -r "user-routes" src/
   grep -r "UserRoutes" src/
   ```

2. **Update Documentation**: Plan to update any documentation that references the router

3. **Test Coverage**: Ensure good test coverage before making changes

### After Renaming

1. **Verify Imports**: Check that all imports are updated correctly
   <CodeTabs type="terminal">
   <Tab title="pnpm">

   ```bash
   pnpm run build  # Should compile without errors
   ```

     </Tab>
     <Tab title="bun">

   ```bash
   bun run build  # Should compile without errors
   ```

     </Tab>
   </CodeTabs>

2. **Run Tests**: Ensure all tests still pass
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

3. **Update Documentation**: Update any documentation that references the router

## Troubleshooting

### Import Resolution Issues

**Error**: TypeScript compilation errors after renaming

<CodeTabs type="terminal">
  <Tab title="pnpm">

```bash
# Clear TypeScript cache
npx tsc --build --clean

# Rebuild project
pnpm run build

# Check for remaining references to old router name
grep -r "OldRouterName" src/
```

  </Tab>
  <Tab title="bun">

```bash
# Clear TypeScript cache
bunx tsc --build --clean

# Rebuild project
bun run build

# Check for remaining references to old router name
grep -r "OldRouterName" src/
```

  </Tab>
</CodeTabs>

### Missing Router References

**Error**: Runtime errors about missing router

**Solution**:

```bash
# Check for missed import updates
grep -r "old-router-name" src/
grep -r "OldRouterName" src/

# Verify registration in index files
cat src/index.ts
cat src/services/*/index.ts
```

### Test File Updates

**Error**: Tests failing after router rename

**Solution**:

1. Rename test files to match new router names
2. Update import statements in test files
3. Update describe blocks and variable names
4. Verify test assertions still make sense

## Related Documentation

- **[Changing Services](./services)** - Service configuration changes
- **[Adding Routers](../adding-projects/routers)** - Creating new routers
- **[Framework Routing](../framework/routing)** - Routing framework documentation
