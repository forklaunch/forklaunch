---
title: "Compliance Overview"
description: "Overview of ForkLaunch's compliance capabilities for SOC 2, HIPAA, and PCI DSS."
category: "Compliance"
---

## Overview

ForkLaunch is designed with compliance-ready infrastructure from the ground up. The platform's architecture: multi-tenancy isolation, encrypted communications, audit logging, and role-based access control, provides a strong foundation for meeting regulatory requirements.

This section covers how ForkLaunch helps you achieve and maintain compliance with common frameworks.

## Supported Frameworks

| Framework | Scope | Documentation |
|-----------|-------|---------------|
| **SOC 2** | Security, availability, confidentiality | [SOC 2 Guide](/docs/compliance/soc2.md) |
| **HIPAA** | Healthcare data protection | [HIPAA Guide](/docs/compliance/hipaa.md) |
| **PCI DSS** | Payment card data security | [PCI DSS Guide](/docs/compliance/pci.md) |

## Built-In Security Controls

ForkLaunch provides several security controls that map to compliance requirements across frameworks:

### Authentication and Authorization

- **JWT-based authentication** with JWKS public key validation for user-facing APIs
- **HMAC authentication** for service-to-service communication with signed requests
- **Role-based access control** with configurable `allowedRoles` on every endpoint
- **Session management** with organization-scoped multi-tenancy

### Data Protection

- **Encryption in transit**: all services communicate over TLS
- **Encryption at rest**: AWS RDS and S3 encryption enabled by default in generated infrastructure
- **Environment variable management**: secrets never hardcoded, loaded from environment configuration
- **Organization isolation**: all queries scoped by `req.session.organizationId`

### Audit and Observability

- **OpenTelemetry integration**: distributed tracing, metrics, and structured logging
- **Request tracing**: every API request gets a trace ID for end-to-end tracking
- **CloudWatch logging**: centralized log aggregation in production
- **Deployment audit trail**: full history of who deployed what and when

### Infrastructure Security

- **VPC isolation**: services run in private subnets
- **IAM least privilege**: generated Pulumi code follows minimum permission principles
- **Container security**: ECS Fargate with no SSH access to containers
- **Network segmentation**: load balancers in public subnets, services in private subnets

## Shared Responsibility

ForkLaunch provides the infrastructure and application framework. You are responsible for:

- **Application-level logic**: input validation, business rule enforcement
- **Data classification**: identifying what data requires special handling
- **Access reviews**: periodic review of who has access to what
- **Incident response**: procedures for handling security events
- **Vendor management**: managing third-party integrations and their compliance

## Getting Started

1. Review the framework-specific guides for your compliance requirements
2. Enable OpenTelemetry observability in your deployment configuration
3. Implement application-level controls specific to your data types
4. Document your compliance posture using the controls ForkLaunch provides

## Related Documentation

- [SOC 2](/docs/compliance/soc2.md): Trust service criteria mapping
- [HIPAA](/docs/compliance/hipaa.md): Healthcare data safeguards
- [PCI DSS](/docs/compliance/pci.md): Payment card data requirements
