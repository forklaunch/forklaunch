---
title: Adding Projects - Libraries
category: Guides
description: Learn how to add and configure libraries in your application.
---

## Add a Library

A **library** is a shared code package that can be imported and used across multiple services and workers in your application. Unlike services and workers that run independently, libraries are code collections (utilities, helpers, shared business logic, or common types) that get bundled into the projects that use them. Think of it as a toolbox: instead of duplicating dice animation logic in every service, you create a `dice-animations-library` that stores animations for different die types (d4, d6, d12, d20) and can be imported by all services.

In ForkLaunch, libraries are simple TypeScript packages with minimal structure. They're perfect for sharing code, types, utilities, and business logic across your application without the overhead of a full service or worker.

## Getting Started

You can add a library in two ways:

### Using Commands (Recommended)

<CodeTabs type="instantiate">
  <Tab title="init">

  ```bash
  forklaunch init library
  ```

  </Tab>
  <Tab title="add">

  ```bash
  forklaunch add library
  ```

  </Tab>
</CodeTabs>

This creates a new library with a simple structure for sharing code across services and workers. All application artifacts are automatically updated.

### Manual Creation with Sync

If you manually create a library directory, use `forklaunch sync` to register it:

```bash
# After manually creating the library structure
forklaunch sync library library-name
```

This updates artifacts (manifest, workspace, tsconfig) to include your manually created library.

**Note**: Libraries should contain reusable functionality that can be imported by other projects in your application.

### Sync Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--path` | `-p` | The application path | Any valid directory path |
| `--prompts` | `-P` | JSON object with pre-provided answers for library options (see init options) | JSON string |

### Init Command Options

| Option | Short | Description | Valid Values |
| :----- | :---- | :---------- | :----------- |
| `--path` | `-p` | The application path to initialize the library in | Any valid directory path |
| `--description` | `-D` | The description of the library | Any string |
| `--dryrun` | `-n` | Dry run the command | Flag (no value) |

### Library Aliases

The `library` command has an alias for convenience:
- `lib`

### Project Structure

A minimal library will have the following structure:

```bash
.
├── eslint.config.mjs -> ../eslint.config.mjs # symlinked from parent for consistency
├── index.ts
├── lib.ts # the library logic
├── package.json
├── tsconfig.json
└── vitest.config.ts -> ../vitest.config.ts # symlinked from parent for consistency
```

### Usage Examples

```bash
# Basic library (shared utilities for dice roll app)
forklaunch init library dice-utils --path ./my-app/src/modules

# Library with custom description
forklaunch init library validation --path ./my-app/src/modules --description "Input validation utilities"

# Library in specific directory
forklaunch init library api-client --path ./shared

# Statistics calculation library
forklaunch init library stats-helpers --path ./my-app/src/modules --description "Statistical calculation utilities"
```

For a complete step-by-step example, see the [Dice Roll Example](/docs/examples/dice-roll-node-app.md).

### Next Steps

After creating a library:
1. Define your library's public API in `index.ts`
2. Implement your core functionality in `lib.ts`
3. Add proper TypeScript types and interfaces
4. Set up unit tests with the configured test framework
5. Configure build settings in `package.json`
6. Import and use the library in your services/workers
7. Document the library's functionality

### Common Issues

1. **Name Conflicts**: Library name cannot be a substring of the application name
2. **Circular Dependencies**: Avoid circular dependencies with other projects
3. **Build Configuration**: Ensure the library builds correctly for consumption
4. **Type Definitions**: Verify TypeScript types are properly exported
5. **Import Paths**: Use correct import paths when consuming the library

### Best Practices

1. **Focused Scope**: Keep the API surface small and focused on a single responsibility
2. **Type Safety**: Include comprehensive TypeScript types for all exports
3. **Documentation**: Write clear documentation and usage examples
4. **Testing**: Write thorough unit tests for all public functions
5. **Dependencies**: Minimize external dependencies to reduce bundle size
6. **Versioning**: Use semantic versioning for library updates
7. **Tree Shaking**: Structure exports to support tree-shaking
8. **Error Handling**: Implement consistent error handling patterns
9. **Shared Utilities**: Focus on truly reusable functionality across projects
