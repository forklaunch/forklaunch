# ForkLaunch

A TypeScript-first backend framework for building typed, modular Node.js services. The monorepo is organized into three main components:

- **framework** - Core runtime libraries: typed HTTP routing (Express/Hyper-Express), schema validation (Zod/TypeBox), DI, OpenTelemetry, universal SDK generation, and infrastructure adapters (Redis, S3).
- **cli** - Command-line tool (`forklaunch`) for scaffolding and managing apps, services, workers, and libraries inside a monorepo.
- **blueprint** - Pre-built, production-ready service templates for common concerns (IAM, billing, workers) that can be generated and customized via the CLI.

# Rules

- Do not use `any` as a type. Use proper TypeScript types instead.
