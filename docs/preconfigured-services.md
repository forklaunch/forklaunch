---
title: Preconfigured Services
category: References
description: Learn about ForkLaunch's pre-built service modules.
---

## Overview

ForkLaunch provides production-ready service modules that handle common application needs. These modules:

- Can be added during initial setup or later
- Ship with opaque implementations for easy updates
- Provide base functionality for extension
- Support ejection into your codebase via `forklaunch eject`

## Available Services

### Identity and Access Management (IAM)
Authorization service that handles:
- User management
- Role-based access control
- Session management

Authentication flavors:
- BetterAuth (https://www.better-auth.com/)
- BYO Authentication (Base IAM + custom code)

[View IAM API Documentation →](https://forklaunch.com/iam-api/docs)

### Billing
Base billing infrastructure service that manages:
- Payment tracking
- Plan creation
- Checkout portal creation
- Subscription management
- Billing portal

Payment Processors:
- Stripe
- BYO Payment Processor (Base Billing + custom code)

[View Billing API Documentation →](https://forklaunch.com/billing-api/docs)

### Ejected Implementation
For full customization, eject the service into your codebase:
```bash
forklaunch eject <service>
```

## Best Practices

1. Start with opaque implementations for faster development
2. Extend base services rather than ejecting when possible
3. Only eject when significant customization is needed
4. Keep custom logic separate from base functionality

## Coming Soon

- Onboarding service: for onboarding users smoothly and seamlessly

More preconfigured services are in development. Follow our [roadmap](/roadmap) for updates.
