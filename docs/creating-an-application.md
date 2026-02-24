---
title: Creating an Application
category: Guides
description: Learn how to create your first ForkLaunch application.
---

## Generating a new Application

To generate a new application, you can use prompts or flags:

```bash
# Interactive prompts
forklaunch init application

# Or with flags (for scripts/AIs)
forklaunch init application my-app \
  --path ./my-app \
  --database postgresql \
  --validator zod \
  --http-framework express \
  --runtime node
```

### Configuration Options

During initialization, you'll be prompted to configure your application. All choices can be modified later:

| Component | Options | Description | How to Change |
| :-------- | :------- | :------------ | :------------- |
| _HTTP Framework_ | `express`, `hyper-express` | HTTP framework for your application | Edit `core/registrations.ts` framework import path |
| _Validator_ | `zod`, `typebox` | Schema validation and coercion library | Edit `core/registrations.ts` validator import path |
| _Runtime_ | `node`, `bun` | Application runtime | Update package scripts and workspace config |
| _Test Runner_ | `vitest`, `jest` | Test execution framework | Update test scripts in package.json files |
| _Database_ | `postgresql`, `mongodb` | Database with MikroORM integration | Update SqlBaseEntity exports and extensions |
| _License_ | `apgl`, `gpl`, `lgpl`, `mozilla`, `apache`, `mit`, `boost`, `unlicense`, `none` | Project license | Edit LICENSE file |
| _Preconfigured Modules_ | `billing`, `iam` | Optional base services (see [Preconfigured Modules](/docs/preconfigured-modules)) | Remove service folder and update manifest.toml |

### Application Architecture
To see a more comprehensive application structure, see the [Architecture documentation](/docs/architecture.md).
**Application Structure:**
```bash
my-app/                    ← Application
├── .forklaunch/
│   └── manifest.toml      ← Application artifact
├── docker-compose.yaml     ← Application artifact
└── src/modules/           ← Projects live here
    ├── api-service/        ← Project (Service)
    ├── email-worker/       ← Project (Worker)
    ├── shared-utils/       ← Project (Library)
    ├── pnpm-workspace.yaml    ← Application artifact
```

### Important Notes

- Requests for more choices are welcome! Please create a discussion on the [ForkLaunch GitHub repository](https://github.com/
forklaunch/forklaunch-js).
- Use `forklaunch change` to make safe changes. If manually editing, use `forklaunch sync` after making changes to help ForkLaunch track metadata
- MikroORM provides both repository patterns and raw SQL access
- Preconfigured services use opaque business logic by default
  - Use `forklaunch eject` to customize logic
  - Ejected services may not receive updates


## Next Steps

Congratulations! You are on your way to creating an application. Choose your path.

### Add Services and Workers
[Adding Projects](/docs/adding-projects.md)

- [Local Development](/docs/local-development.md) - Run your application locally
- [Framework Reference](/docs/framework.md) - Learn about framework features
- [CLI Reference](/docs/cli.md) - Explore available commands
- [Customizations](/docs/customizations.md) - Customize your application
<!-- - [Deployment](/docs/deployment) - Deploy your application -->

