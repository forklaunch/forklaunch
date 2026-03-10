---
title: "HIPAA Compliance"
description: "How ForkLaunch supports HIPAA compliance for applications handling protected health information."
category: "Compliance"
---

## Overview

HIPAA (Health Insurance Portability and Accountability Act) requires safeguards for Protected Health Information (PHI). ForkLaunch's architecture provides technical controls that support HIPAA compliance, but achieving full compliance requires organizational policies and procedures beyond what any framework provides.

**Important:** Using ForkLaunch does not automatically make your application HIPAA-compliant. You must implement application-level controls, execute a Business Associate Agreement (BAA) with AWS, and establish organizational policies.

## HIPAA Safeguards

### Technical Safeguards

| Requirement | ForkLaunch Capability |
|------------|----------------------|
| **Access Control (§164.312(a))** | JWT authentication, role-based authorization, organization-scoped multi-tenancy |
| **Audit Controls (§164.312(b))** | OpenTelemetry distributed tracing, CloudWatch centralized logging, request-level trace IDs |
| **Integrity (§164.312(c))** | Schema validation on all endpoints, database transactions via MikroORM Unit of Work |
| **Transmission Security (§164.312(e))** | TLS on all communications, HTTPS enforcement via ALB |
| **Authentication (§164.312(d))** | JWKS-based JWT validation, HMAC service-to-service authentication |

### Physical Safeguards

Physical safeguards are handled by AWS when you deploy to their infrastructure:

- **Facility access controls**: AWS data center security
- **Workstation security**: ECS Fargate containers with no SSH access
- **Device and media controls**: AWS handles hardware lifecycle

You must execute an AWS BAA to cover physical safeguards.

### Administrative Safeguards

Administrative safeguards are organizational responsibilities. ForkLaunch supports them with:

| Requirement | How ForkLaunch Helps |
|------------|---------------------|
| **Risk Analysis** | Infrastructure as Code makes your architecture auditable and reviewable |
| **Workforce Training** | Consistent patterns and conventions reduce configuration errors |
| **Contingency Plan** | Pulumi IaC enables full environment recreation from code |
| **Access Management** | Role-based access control built into every endpoint |

## PHI Data Handling

### Encryption

ForkLaunch's generated AWS infrastructure includes:

- **RDS encryption at rest**: AES-256 encryption for database storage
- **S3 server-side encryption**: automatic encryption for stored objects
- **ElastiCache encryption**: encryption at rest and in transit
- **TLS everywhere**: all service-to-service and client-to-server communication

### Data Isolation

- **Organization-scoped queries**: all database queries filtered by `req.session.organizationId`
- **VPC isolation**: services in private subnets, not directly accessible from the internet
- **Environment separation**: separate configurations for development, staging, production

### Audit Logging

```typescript
// Every request is automatically traced via OpenTelemetry
// Traces include: who made the request, what was accessed, when, and from where

// For PHI-specific audit logging, add application-level logging:
logger.info('PHI accessed', {
  userId: req.session.userId,
  organizationId: req.session.organizationId,
  resource: 'patient-record',
  resourceId: patientId,
  action: 'read'
});
```

## Implementation Checklist

### AWS Configuration

- [ ] Execute AWS Business Associate Agreement (BAA)
- [ ] Enable AWS CloudTrail for API-level audit logging
- [ ] Enable RDS encryption at rest
- [ ] Enable S3 bucket encryption
- [ ] Configure VPC flow logs

### Application Controls

- [ ] Identify all PHI data fields in your entities
- [ ] Implement PHI-specific audit logging beyond default traces
- [ ] Configure data retention and destruction policies
- [ ] Implement minimum necessary access: limit PHI to roles that need it
- [ ] Set up automated session timeouts
- [ ] Implement emergency access procedures

### Organizational Policies

- [ ] Designate a HIPAA Security Officer
- [ ] Document risk analysis and management procedures
- [ ] Establish workforce training program
- [ ] Create incident response and breach notification procedures
- [ ] Execute BAAs with all subcontractors and vendors
- [ ] Establish policies for PHI disposal

## Related Documentation

- [Compliance Overview](/docs/compliance/overview.md)
- [SOC 2](/docs/compliance/soc2.md)
- [PCI DSS](/docs/compliance/pci.md)
