---
title: Adding Services and Workers
category: Guides
description: Add services, workers, libraries, routers, and modules to your ForkLaunch application.
---

# Overview

**Projects** are the building blocks of your ForkLaunch application. They represent different types of components that work together to form a complete system. Each project type serves a specific purpose and can be added to your application using the `forklaunch init` command.

## Quickstart

### Modules
Preconfigured, production-ready services that provide common functionality:

```bash
# Interactive mode
forklaunch init module

# Add billing module
forklaunch init module billing --path ./my-app --module billing-stripe --database postgresql

# Add IAM module
forklaunch init module auth --path ./my-app --module iam-base --database postgresql
forklaunch init module auth --path ./my-app --module iam-better-auth --database postgresql
```

### Services
Self-contained API services that handle specific business domains:

```bash
# Basic service
forklaunch init service users --path ./my-app/src/modules --database postgresql

# Service with Redis cache
forklaunch init service products --path ./my-app/src/modules --database postgresql --infrastructure redis

# Service with multiple infrastructure
forklaunch init service files --path ./my-app/src/modules --database postgresql --infrastructure redis s3
```

### Workers
Background processes for asynchronous job processing:

```bash
# Database worker
forklaunch init worker email-processor --path ./my-app/src/modules --type database --database postgresql

# Redis worker
forklaunch init worker notification-worker --path ./my-app/src/modules --type redis

# BullMQ worker
forklaunch init worker scheduled-jobs --path ./my-app/src/modules --type bullmq

# Kafka worker
forklaunch init worker analytics-consumer --path ./my-app/src/modules --type kafka
```

### Libraries
Shared code and utilities used across services and workers:

```bash
# Basic library
forklaunch init library utils --path ./my-app/src/modules

# Library with description
forklaunch init library validation --path ./my-app/src/modules --description "Input validation utilities"
```

### Routers
Add new routes and controllers to existing services:

```bash
# Basic router
forklaunch init router products --path ./my-app/src/modules/my-service

# Router with infrastructure
forklaunch init router orders --path ./my-app/src/modules/commerce-service --infrastructure redis
```

### Definitions

| Concept | Definition | What It Does | Common Examples |
|----------|-------------|---------------|------------------|
| **Module** | A **preconfigured, production-ready service** that provides common functionality out of the box. | It serves as a plug-and-play solution for standard features (like authentication or billing), with best practices and integrations already built in, saving you from building these from scratch. | - A **billing module** with Stripe integration for payments and subscriptions.<br>- An **IAM module** for user authentication, authorization, and session management.<br>- An **email module** for transactional emails and notifications. |
| **Service** | A **self-contained API component** that handles synchronous requests for a specific business domain. | It runs continuously as an HTTP server and responds to requests in real-time (from users, other services, or front-end apps). Often paired with a worker to offload heavy or async tasks. | - An **API service** that handles user logins and profiles.<br>- A **billing service** that manages payments.<br>- A **data ingestion service** that collects and processes data. |
| **Worker** | A **background process** that handles asynchronous tasks for a specific business domain. | It processes jobs from a queue or schedule, performing work that doesn't need immediate responses, often handling heavier workloads, scheduled tasks, or work offloaded from a paired service. | - Sending emails after a signup.<br>- Generating daily reports.<br>- Processing large datasets.<br>- Cleaning up logs or cache files. |
| **Library** | A **shared collection of code**, utilities, or models used across services and workers. | Provides reusable logic (like validation, database models, or helper functions) that multiple parts of the system can import. | - **Core library** with shared models and validation schemas.<br>- **Auth library** with JWT and session utilities.<br>- **Utils library** for formatting, logging, or constants. |
| **Router** | A **set of routes and controllers** that extends an existing service with new endpoints. | It adds new API routes and their handling logic to a service without creating a separate service, allowing you to organize and grow your service's functionality modularly. | - A **products router** in an e-commerce service to manage product CRUD operations.<br>- An **orders router** to handle order processing and tracking.<br>- An **analytics router** to expose reporting endpoints. |

**Note:** Services and workers share architecture; they are designed to work together as needed.




## Next Steps

Learn more about each project type:
- [Modules](/docs/adding-projects/modules.md)
- [Services](/docs/adding-projects/services.md)
- [Workers](/docs/adding-projects/workers.md)
- [Libraries](/docs/adding-projects/libraries.md)
- [Routers](/docs/adding-projects/routers.md)
