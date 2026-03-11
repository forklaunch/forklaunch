---
title: "PCI DSS Compliance"
description: "How ForkLaunch supports PCI DSS compliance for applications handling payment card data."
category: "Compliance"
---

## Overview

PCI DSS (Payment Card Industry Data Security Standard) applies to any organization that stores, processes, or transmits cardholder data. ForkLaunch integrates with Stripe for payment processing, which significantly reduces your PCI scope by keeping card data off your servers entirely.

## PCI Scope with Stripe

ForkLaunch's billing module uses Stripe as the payment processor. With Stripe's client-side tokenization (Stripe Elements or Checkout), cardholder data never touches your servers. This places most ForkLaunch applications under **SAQ A** or **SAQ A-EP**, the simplest PCI compliance levels.

| Approach | PCI Level | Card Data on Your Server |
|----------|-----------|--------------------------|
| Stripe Checkout (redirect) | SAQ A | Never |
| Stripe Elements (embedded) | SAQ A-EP | Never (tokenized client-side) |
| Direct API (raw card numbers) | SAQ D | Yes; avoid this |

**Recommendation:** Use Stripe Checkout or Stripe Elements. ForkLaunch's billing module is designed for this pattern.

## PCI DSS Requirement Mapping

### Requirement 1: Network Security

| Control | ForkLaunch Capability |
|---------|----------------------|
| Install and maintain firewalls | VPC security groups, private subnets for services |
| No vendor-supplied defaults | Generated infrastructure uses unique credentials via environment variables |

### Requirement 2: Secure Configuration

| Control | ForkLaunch Capability |
|---------|----------------------|
| System hardening | ECS Fargate: no OS to harden, managed container runtime |
| Minimal services | Each module runs only what it needs; no unnecessary services |

### Requirement 3: Protect Stored Data

| Control | ForkLaunch Capability |
|---------|----------------------|
| Encryption at rest | RDS encryption, S3 encryption enabled in generated infrastructure |
| No cardholder data storage | Stripe handles all card data; only Stripe customer/subscription IDs stored locally |

### Requirement 4: Encrypt Transmissions

| Control | ForkLaunch Capability |
|---------|----------------------|
| TLS for public networks | ALB terminates TLS, HTTPS enforced on all public endpoints |
| Encrypted service communication | All internal service communication over TLS |

### Requirement 5: Anti-Malware

| Control | ForkLaunch Capability |
|---------|----------------------|
| Malware protection | ECS Fargate: immutable container images, no persistent filesystem |

### Requirement 6: Secure Development

| Control | ForkLaunch Capability |
|---------|----------------------|
| Secure development practices | Schema validation on all inputs, typed contracts, no raw SQL |
| Vulnerability management | npm/pnpm audit, dependency management via lockfiles |

### Requirement 7: Restrict Access

| Control | ForkLaunch Capability |
|---------|----------------------|
| Need-to-know access | Role-based authorization (`allowedRoles`) on every endpoint |
| Organization isolation | Multi-tenancy scoped by `req.session.organizationId` |

### Requirement 8: Identify Users

| Control | ForkLaunch Capability |
|---------|----------------------|
| Unique user identification | JWT-based authentication with unique session tokens |
| Strong authentication | JWKS public key validation, configurable auth requirements |

### Requirement 10: Logging and Monitoring

| Control | ForkLaunch Capability |
|---------|----------------------|
| Audit trails | OpenTelemetry distributed tracing on all requests |
| Centralized logging | CloudWatch log aggregation with structured logs |
| Tamper detection | CloudWatch Logs with log group retention policies |

### Requirement 11: Testing

| Control | ForkLaunch Capability |
|---------|----------------------|
| Vulnerability scanning | TestContainers for integration testing, CI/CD pipeline support |
| Penetration testing | Architecture documentation via IaC for scope definition |

### Requirement 12: Security Policies

Security policies are organizational responsibilities; ForkLaunch provides the technical controls referenced in your policies.

## Implementation Checklist

### Stripe Integration

- [x] Use Stripe Checkout or Stripe Elements (no raw card handling)
- [x] Store only Stripe customer IDs and subscription IDs locally
- [ ] Configure Stripe webhook signature verification
- [ ] Enable Stripe Radar for fraud detection

### Infrastructure

- [x] TLS on all endpoints
- [x] VPC with private subnets for services
- [x] Encrypted database storage
- [x] No cardholder data in logs (Stripe handles tokenization)
- [ ] Enable VPC flow logs for network monitoring
- [ ] Configure CloudWatch alarms for suspicious activity

### Application

- [x] Input validation on all endpoints via schema validator
- [x] Role-based access control
- [x] Organization-scoped data isolation
- [ ] Review and restrict IAM permissions
- [ ] Implement session timeout policies
- [ ] Document your cardholder data flow (should show Stripe handling all card data)

## Related Documentation

- [Compliance Overview](/docs/compliance/overview.md)
- [SOC 2](/docs/compliance/soc2.md)
- [HIPAA](/docs/compliance/hipaa.md)
