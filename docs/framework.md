---
title: Framework
category: References
description: Reference for the ForkLaunch framework.
---

## Overview

The `ForkLaunch` framework collects a set of lightweight libraries and exposes useful utilities that make API first development possible.

## Core Libraries

### HTTP and Routing

- `@forklaunch/express`: Express.js integration layer providing routing and middleware support
- `@forklaunch/hyper-express`: HyperExpress integration layer for high-performance HTTP handling

### Validation and Types

- `@forklaunch/validator`: Schema validation and type coercion library supporting Zod and TypeBox
- `@forklaunch/universal-sdk`: Type-safe SDK generator for consuming ForkLaunch APIs from any platform

### Utilities

- `@forklaunch/common`: Common HTTP utilities, middleware, and request/response handling
- `@forklaunch/core`: Core framework library containing shared utilities, base classes, and common interfaces
- `@forklaunch/internal`: Internal utilities for ForkLaunch framework

## Framework Features

- [HTTP Frameworks](/docs/framework/http.md)
- [Validation](/docs/framework/validation.md)
- [Observability and OpenTelemetry](/docs/framework/telemetry.md)
- [Configuration](/docs/framework/config.md)
- [Authorization](/docs/framework/authorization.md)
- [OpenAPI + Documentation](/docs/framework/documentation.md)
- [MCP Generation](/docs/framework/mcp.md)
- [Error Handling](/docs/framework/error-handling.md)
- [Universal SDK](/docs/framework/universal-sdk.md)
