<!-- TODO: write "Quickstart: Hello World", a self-contained step-by-step guide to launching a first service from scratch. Decide where it should live in the docs nav once ready. -->
---
title: Quickstart
category: Get Started
description: Common CLI commands for creating, modifying, and managing ForkLaunch projects.
---
<!-- TODO: write “Quickstart: Hello World” and decide where it should live in docs nav -->

## Common Workflow

These commands cover the most frequent operations. Run any command with `--help` for all available flags.

**Create a new project:**

```bash
forklaunch init app my-app --database postgresql --runtime node
```

**Add components:**

```bash
forklaunch init service billing --database postgresql
forklaunch init worker email-processor --type bullmq
forklaunch init router api-v1
forklaunch init library utils
```

**Modify components:**

```bash
forklaunch change service billing --database mysql
forklaunch change worker email-processor --type kafka
forklaunch change router api-v1 --new-name api-v2
forklaunch change library utils --description "Shared utilities"
```

**Remove components:**

```bash
forklaunch delete service old-billing
forklaunch delete worker deprecated-processor
forklaunch delete router legacy-api
forklaunch delete library unused-utils
```

**Sync components:**

After making manual changes to the repo, run `sync` to align artifacts and update dependecies:

```bash
forklaunch sync all
forklaunch sync service new-billing
forklaunch sync worker custom-processor
forklaunch sync library new-utils
```

**Development utilities:**

```bash
forklaunch depcheck          # Check for missing or unused dependencies
forklaunch eject             # Take ownership of preconfigured module implementations
forklaunch config --show     # Print current CLI configuration
```

**Platform integration:**

```bash
forklaunch login             # Authenticate with the ForkLaunch platform
forklaunch whoami            # Print the currently authenticated user
forklaunch logout            # Log out
```

## Next Steps

- [Getting Started](/docs/getting-started.md): Install the CLI and create your first application
- [Creating an Application](/docs/creating-an-application.md): Full step-by-step walkthrough
- [CLI Reference](/docs/cli.md): Complete command reference with all flags
