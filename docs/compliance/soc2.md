---
title: "SOC 2 Compliance"
description: "How ForkLaunch's architecture maps to SOC 2 Trust Service Criteria."
category: "Compliance"
---

## Overview

SOC 2 (System and Organization Controls 2) evaluates an organization's controls across five Trust Service Criteria: Security, Availability, Processing Integrity, Confidentiality, and Privacy. ForkLaunch's architecture provides built-in controls that map to many SOC 2 requirements.

## Trust Service Criteria Mapping

### Security (Common Criteria)

The Security criteria forms the foundation of SOC 2. ForkLaunch addresses these controls through:

| Control Area | ForkLaunch Capability |
|-------------|----------------------|
| **Access Control** | JWT authentication with JWKS validation, role-based authorization on every endpoint, HMAC for service-to-service auth |
| **Network Security** | VPC with private subnets, ALB for traffic management, security groups limiting ingress/egress |
| **Change Management** | Git-based deployment workflow, Pulumi IaC for infrastructure changes, deployment audit trail |
| **Monitoring** | OpenTelemetry tracing and metrics, CloudWatch log aggregation, health check endpoints |
| **Logical Access** | Organization-scoped multi-tenancy (`req.session.organizationId`), permission checks on all endpoints |

### Availability

| Control Area | ForkLaunch Capability |
|-------------|----------------------|
| **Redundancy** | ECS Fargate with configurable task counts, RDS Multi-AZ support, ElastiCache replication |
| **Disaster Recovery** | Infrastructure as Code (Pulumi) enables full environment recreation, RDS automated backups |
| **Capacity Planning** | Auto-scaling via ECS service configuration, CloudWatch alarms for resource utilization |
| **Incident Management** | Structured logging with trace IDs, deployment rollback via redeployment |

### Confidentiality

| Control Area | ForkLaunch Capability |
|-------------|----------------------|
| **Encryption in Transit** | TLS on all service communication, HTTPS enforcement via ALB |
| **Encryption at Rest** | RDS encryption, S3 server-side encryption, ElastiCache encryption |
| **Data Isolation** | Organization-scoped queries, separate environment configurations, VPC network isolation |
| **Secret Management** | Environment variable configuration, no hardcoded secrets in code |

### Processing Integrity

| Control Area | ForkLaunch Capability |
|-------------|----------------------|
| **Input Validation** | Schema validation on every endpoint via `schemaValidator`, typed request/response contracts |
| **Error Handling** | Standardized error responses, structured error logging, job retry with DLQ |
| **Data Consistency** | MikroORM Unit of Work pattern, database transactions, migration versioning |

### Privacy

Privacy controls are primarily application-specific. ForkLaunch provides the foundation:

- **Data access logging** via OpenTelemetry traces
- **Multi-tenancy isolation** prevents cross-organization data access
- **Role-based access** limits data visibility based on user permissions

## Implementation Checklist

### Infrastructure Controls (Provided by ForkLaunch)

- [x] TLS encryption on all endpoints
- [x] VPC with public/private subnet separation
- [x] IAM roles with least privilege
- [x] Encrypted database storage
- [x] Centralized logging
- [x] Distributed tracing
- [x] Authentication on all endpoints
- [x] Organization-scoped data isolation

### Application Controls (Your Responsibility)

- [ ] Define data classification policies
- [ ] Implement data retention and deletion procedures
- [ ] Configure alerting thresholds in CloudWatch
- [ ] Set up access review processes
- [ ] Document incident response procedures
- [ ] Implement application-specific audit logging for sensitive operations
- [ ] Configure backup retention policies

## Related Documentation

- [Compliance Overview](/docs/compliance/overview.md)
- [HIPAA](/docs/compliance/hipaa.md)
- [PCI DSS](/docs/compliance/pci.md)
