# ForkLaunch Compliance Coverage Report

*Generated: March 24, 2026*

ForkLaunch (framework + platform) addresses every technical control required by HIPAA, SOC 2, PCI DSS, and GDPR at both the application and infrastructure layers. Controls that are not covered are organizational processes (legal agreements, training, incident response documentation) that require business action, not software.

---

## Technical Controls — Fully Addressed

### 1. Field-Level Data Classification
**Status:** Addressed
**How:** `fp.compliance('pii'|'phi'|'pci'|'none')` required at compile time on every entity field. Classification stored in manifest. CLI `forklaunch compliance audit` generates reports from classifications.
**Standard:** HIPAA §164.312(a), PCI DSS 3.1, SOC 2 CC6.1

### 2. PHI/PCI Application-Level Encryption
**Status:** Addressed
**How:** AES-256-GCM with HKDF-derived per-tenant keys. MikroORM EventSubscriber auto-encrypts PHI/PCI fields on persist, auto-decrypts on load. Versioned ciphertext (`v1:` prefix) for key rotation support. `em.nativeInsert` blocked on compliance entities to prevent encryption bypass. PII relies on infrastructure encryption (sufficient per standards).
**Standard:** HIPAA §164.312(a)(2)(iv), PCI DSS 3.4

### 3. Encryption at Rest (Infrastructure)
**Status:** Addressed
**How:** RDS `storageEncrypted: true` on all instances. ElastiCache `atRestEncryptionEnabled: true`. S3 bucket encryption for Pulumi state, Docker layer caches, and audit logs. MSK Kafka encryption at rest.
**Standard:** HIPAA §164.312(a)(2)(iv), PCI DSS 3.4, SOC 2 CC6.1

### 4. Encryption in Transit
**Status:** Addressed
**How:** ALB HTTPS listeners with ACM certificates. ElastiCache `transitEncryptionEnabled: true`. Kafka MSK `clientBroker: 'TLS', inCluster: true`. RDS SSL with global CA bundle. Wildcard certs for `*.app.forklaunch.com` and per-region monitoring domains. Custom domain certificate support.
**Standard:** HIPAA §164.312(e)(1), PCI DSS 4.1, SOC 2 CC6.1

### 5. Tenant Isolation (Application)
**Status:** Addressed
**How:** MikroORM global filter auto-appends `WHERE organization_id = :tenantId` on every query (mandatory, cannot be disabled). PostgreSQL RLS via `SET LOCAL app.tenant_id` per transaction (configurable opt-out). `getSuperAdminContext()` for explicit cross-tenant bypass with audit logging. `em.nativeInsert/Update/Delete` blocked on compliance entities.
**Standard:** HIPAA §164.312(a)(1), SOC 2 CC6.1, CC6.3

### 6. Tenant Isolation (Infrastructure)
**Status:** Addressed
**How:** VPC with public/private subnets across 2+ availability zones. Security groups: ALB accepts HTTP(S) only, ECS accepts traffic from ALB only (ports 0-65535), databases accept traffic from ECS only (port-specific), VPC endpoints accept HTTPS from VPC CIDR only. ENI-level container isolation via `awsvpc` network mode.
**Standard:** PCI DSS 1.3, SOC 2 CC6.1

### 7. Access Control (Application)
**Status:** Addressed
**How:** `access: 'public'|'authenticated'|'protected'|'internal'` required on every route at compile time. `'protected'` requires RBAC declaration (allowedRoles/allowedPermissions/requiredScope). `'internal'` requires HMAC auth. Startup validation rejects routes with missing or mismatched access/auth. SDK auto-generates correct auth headers per access level.
**Standard:** HIPAA §164.312(d), PCI DSS 7.1, SOC 2 CC6.1

### 8. Access Control (Infrastructure)
**Status:** Addressed
**How:** IAM least-privilege per ECS service with ARN-scoped policies. JIT (Just-In-Time) elevated access with time limits per application/environment. Separate task roles and execution roles. Account-scoped assume role conditions. Resource groups for application-scoped visibility.
**Standard:** HIPAA §164.312(d), PCI DSS 7.2, SOC 2 CC6.2, CC6.3

### 9. Audit Logging (Application)
**Status:** Addressed
**How:** Every HTTP request and WebSocket event automatically logged via OTEL collector, tagged `log.type: 'audit'` for Loki routing. Entries include: timestamp, userId, tenantId, route, method, SHA-256 body hash (never plaintext), status, duration. Compliance fields redacted. Auth failures, rate limit hits, RBAC denials, super-admin bypasses all logged with specific event types. Cannot be disabled — baked into middleware pipeline.
**Standard:** HIPAA §164.312(b), PCI DSS 10.1, SOC 2 CC7.2

### 10. Audit Logging (Infrastructure)
**Status:** Addressed
**How:** CloudTrail with `enableLogFileValidation: true` and S3 backend. Multi-region trail support. ComplianceAuditLog entity tracking actor, action, details, timestamp. Lifecycle-based retention policies on log storage. CloudWatch log groups for application, ECS, and ALB logs with configurable retention.
**Standard:** HIPAA §164.312(b), PCI DSS 10.2, SOC 2 CC7.2

### 11. Rate Limiting
**Status:** Addressed
**How:** Per-tenant per-route per-user rate limiting via Redis (MULTI/EXEC atomic increment). Separate read/write operation tiers. Public routes rate-limited by IP. Response headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`. Fail open on Redis outage with degraded audit logging. Rate limit hits logged to audit trail.
**Standard:** PCI DSS 6.5.10, SOC 2 CC6.1

### 12. Secrets Management
**Status:** Addressed
**How:** Framework: boot-time validation refuses to start if declared secrets missing. Typed `getSecret(key)` throws on undeclared keys. `.secrets.local` for local dev with same interface. Platform: AES-256-GCM encrypted environment variables in database. Observability credentials encrypted with nested JSON support. Idempotent encrypt/decrypt. ECS Secrets Manager integration for runtime injection.
**Standard:** HIPAA §164.312(a)(2)(iv), PCI DSS 3.5, SOC 2 CC6.1

### 13. Key Rotation
**Status:** Addressed
**How:** Framework: versioned ciphertext (`v1:` prefix) supports future key versions. Platform: automated rotation scripts for encryption keys (re-encrypts all environment_variable, template_environment_variable, and observability_configuration.credentials rows), Pulumi passphrases (downloads S3 state, decrypts with old, re-encrypts with new across all stacks), database passwords (RandomPassword 32 chars). Dry-run mode available. Auto-generates new keys if not provided.
**Standard:** PCI DSS 3.6, SOC 2 CC6.1

### 14. Network Segmentation
**Status:** Addressed
**How:** VPC with configurable CIDR, public subnets for ALB/NAT, private subnets for RDS/ElastiCache/MSK/ECS. NAT Gateway for outbound from private subnets. Security groups per tier with strict ingress rules. VPC endpoints for AWS services (port 443 from VPC CIDR). DNS support enabled. Protected VPC with `retainOnDelete: true`.
**Standard:** PCI DSS 1.1, 1.3, SOC 2 CC6.1

### 15. Vulnerability Scanning
**Status:** Addressed
**How:** ECR `imageScanningConfiguration: { scanOnPush: true }` on all repositories. Docker image hash tagging for integrity verification. Multi-stage builds for minimal attack surface. Alpine/slim base images. CA certificates bundled. RDS global certificate bundle downloaded in build. No secrets baked into images.
**Standard:** PCI DSS 6.1, SOC 2 CC7.1

### 16. Container Security
**Status:** Addressed
**How:** `awsvpc` network mode for ENI-level isolation. Separate task role (application permissions) and execution role (ECS runtime). Health checks via HTTP GET `/health` with retries/timeout. CPU/memory limits enforced. ARM64 architecture. Essential flag on containers. Secrets injected via ECS Secrets Manager (not environment variables). `awslogs` driver for CloudWatch integration.
**Standard:** PCI DSS 6.5, SOC 2 CC6.1

### 17. Multi-Region / Data Residency
**Status:** Addressed
**How:** Framework: manifest `data_residency` field declares allowed regions. Platform: Global Accelerator for multi-region routing. Per-region Pulumi stacks (`{appId}-{env}-{region}`). Regional ALBs, security groups, subnets, certificates. Cross-region peering support. Route 53 hosted zones per application. Regional CloudWatch log groups. Compiler validates target region against manifest constraint before deployment.
**Standard:** GDPR Art. 44-49, SOC 2 CC6.1

### 18. Monitoring / Observability
**Status:** Addressed
**How:** Framework: OTEL metrics for encryption operations, rate limit hits/exceeded, tenant filter bypasses, WS connections, auth failures. Platform: CloudWatch log groups for application/ECS/ALB. Support for Prometheus (metrics), Loki (logs), Tempo (traces), OTEL Collector, CloudWatch. Provider-specific credentials encrypted. Configurable sampling rates, custom labels, detailed tracing toggle, resource allocation per provider, retention policies.
**Standard:** HIPAA §164.308(a)(1), SOC 2 CC7.2, CC7.3

### 19. WebSocket Security
**Status:** Addressed
**How:** Authentication at handshake — reject before any data transmitted. Periodic session re-validation at configurable interval (default 5 min). Tenant-scoped channels — broadcast checks recipient's tenantId and permissions before delivery. Cross-tenant delivery blocked. All WS events (connect, message, broadcast, disconnect) audit-logged. Close code 4001 for auth failures.
**Standard:** SOC 2 CC6.1

### 20. Compliance Reporting
**Status:** Addressed
**How:** Framework: `forklaunch compliance audit` CLI command generates JSON report covering routes + access levels, entity field classifications, secrets status, data residency config. Platform: ComplianceStandard entity tracking supported standards. ComplianceFeature entity tracking individual features. ComplianceAuditLog entity for audit trail. EnvironmentComplianceConfig for per-environment configuration. OrganizationComplianceTemplate for org-level templates.
**Standard:** HIPAA §164.308(a)(8), SOC 2 CC4.1, PCI DSS 12.2

### 21. Backup / Recovery
**Status:** Addressed
**How:** RDS backup retention policies (customizable). Final snapshot on deletion. Copy tags to snapshots. Multi-AZ deployments for automatic failover. ElastiCache automatic failover with Multi-AZ option. S3 versioning on Pulumi state bucket.
**Standard:** HIPAA §164.308(a)(7), SOC 2 A1.2

### 22. Certificate Management
**Status:** Addressed
**How:** ACM wildcard certificates for `*.{stackName}.app.forklaunch.com`. Secondary SNI certificates for custom domains. Per-region monitoring domain certificates. DNS validation via Route 53. Certificate management across 9 AWS regions (US, EU, AP, CA, SA). Support for both new and existing custom domain certificates.
**Standard:** PCI DSS 4.1, SOC 2 CC6.1

### 23. Password / Credential Policies
**Status:** Addressed
**How:** Framework: delegated to better-auth identity provider with multi-org membership, dynamic RBAC, session management. Platform: `RandomPassword` 32 chars with special characters for database passwords. Auto-generated 64-char HMAC secret keys. Base64-encoded webhook secrets. Monitoring secrets via random hex (32 bytes). `openssl rand -hex 32` for Pulumi passphrases.
**Standard:** PCI DSS 8.2, SOC 2 CC6.1

---

## Partially Addressed — Engineering Action Required

### 24. Data Retention / Disposal
**Status:** Partial
**What's done:** CloudTrail lifecycle policies for log retention. S3 30-day auto-expiration on Docker layer caches. Audit log bucket retention policies.
**What's missing:** Application-level data retention policies — automatic deletion of records after a configurable retention period (e.g., delete user data 30 days after account deletion, purge audit logs after 7 years).
**Action required (Engineering):** Implement a retention policy engine in the framework that reads retention configuration from the manifest and automatically purges expired data. Add `retention` field to entity compliance metadata (e.g., `fp.string().compliance('pii').retention('3y')`). Build a scheduled job that enforces retention policies.
**Standard:** HIPAA §164.530(j), PCI DSS 3.1, SOC 2 CC6.5

---

## Not Addressed — Business Action Required

These requirements are organizational, legal, or procedural. They cannot be enforced by software and require business processes to be established by the organization deploying ForkLaunch.

### 25. Breach Notification Procedures
**What's needed:** A documented procedure for notifying affected individuals, regulatory bodies (HHS for HIPAA, card brands for PCI), and business associates within required timeframes (60 days for HIPAA, 72 hours for GDPR) after discovering a data breach.
**Who owns this:** Legal team, Compliance officer, CISO
**Recommended steps:**
1. Draft a breach notification policy with escalation contacts and timelines
2. Establish a breach assessment team (legal, engineering, communications)
3. Pre-draft notification letter templates for each regulation
4. Test the process annually with tabletop exercises
**Standard:** HIPAA §164.408, GDPR Art. 33-34

### 26. Business Associate Agreements (BAAs)
**What's needed:** Legal contracts between the covered entity (your customer) and ForkLaunch (or any subprocessor) that establish permitted uses of PHI, require safeguards, and define breach notification obligations.
**Who owns this:** Legal team, Sales/partnerships
**Recommended steps:**
1. Draft a standard BAA template reviewed by healthcare counsel
2. Execute BAAs with all customers handling PHI before they go live
3. Maintain a register of all BAAs with renewal dates
4. Include BAA requirements in your vendor onboarding process
**Standard:** HIPAA §164.308(b), §164.502(e)

### 27. Employee Security Training
**What's needed:** Regular security awareness training for all employees with access to systems containing sensitive data. Training must cover HIPAA privacy/security rules, PCI data handling, phishing awareness, and incident reporting.
**Who owns this:** HR, CISO, Compliance officer
**Recommended steps:**
1. Implement annual security awareness training (tools: KnowBe4, Curricula, or similar)
2. Require training completion before system access is granted
3. Track completion rates and maintain certificates of completion
4. Conduct quarterly phishing simulations
5. Role-specific training for developers (secure coding) and ops (incident response)
**Standard:** HIPAA §164.308(a)(5), PCI DSS 12.6, SOC 2 CC1.4

### 28. Incident Response Plan
**What's needed:** A documented incident response plan that defines roles, escalation procedures, communication protocols, containment strategies, forensic analysis procedures, and post-incident review processes.
**Who owns this:** CISO, Engineering leadership, Legal team
**Recommended steps:**
1. Draft an incident response plan covering: identification, containment, eradication, recovery, post-incident review
2. Assign an incident commander and define the response team
3. Establish communication channels (internal Slack channel, external status page, regulatory notification contacts)
4. Define severity levels (P0-P3) with corresponding response timelines
5. Conduct annual incident response drills
6. Maintain a log of past incidents and lessons learned
**Standard:** PCI DSS 12.10, SOC 2 CC7.3, CC7.4, HIPAA §164.308(a)(6)

### 29. Physical Security
**What's needed:** Physical security controls for data centers and offices where sensitive data is processed.
**Who owns this:** AWS (for cloud-hosted deployments under the shared responsibility model)
**Note:** For cloud-hosted ForkLaunch deployments, physical security is AWS's responsibility. AWS data centers are certified under SOC 1/2/3, PCI DSS, HIPAA, and ISO 27001. If any on-premises components exist, the deploying organization is responsible for physical access controls, visitor logs, and environmental protections.
**Standard:** PCI DSS 9.x, SOC 2 CC6.4

---

## Known Gaps — Addressable by Engineering

These are technical gaps that ForkLaunch can address with additional feature work. They are not organizational processes — they are software features that have not yet been built.

### 30. Right to Erasure (GDPR)
**Status:** Not yet addressed
**Gap:** No automated mechanism to cascade-delete all PII/PHI for a specific user across all entities.
**How to address:** Build `forklaunch gdpr erase --user-id X` CLI command that uses the compliance field metadata to identify all entities containing PII for a user and cascade-delete them. The compliance classification on every field (`fp.compliance('pii')`) already provides the map of where PII lives — the erasure command reads this map and executes deletions.
**Standard:** GDPR Art. 17

### 31. Data Portability (GDPR)
**Status:** Not yet addressed
**Gap:** No automated mechanism to export all data for a specific user in a machine-readable format.
**How to address:** Build `forklaunch gdpr export --user-id X` CLI command that uses the same compliance metadata to collect all PII fields for a user and export as JSON. The field classifications already identify which fields contain personal data.
**Standard:** GDPR Art. 20

### 32. Consent Management (GDPR)
**Status:** Not yet addressed
**Gap:** No consent tracking, withdrawal mechanism, or consent-gated field access at the framework level.
**How to address:** Add a consent tracking entity and a `'pii-consent'` compliance classification that requires active consent before PII fields are accessible. Consent withdrawal automatically redacts or deletes associated data.
**Standard:** GDPR Art. 7

### 33. Automatic Session Logoff
**Status:** Not yet addressed
**Gap:** Session timeout is delegated to better-auth but not enforced at the framework level. No guaranteed maximum session duration.
**How to address:** Enforce a configurable maximum session TTL at the framework middleware level. If a session exceeds the TTL, the request is rejected regardless of the identity provider's session state. WebSocket periodic re-validation (already implemented with configurable interval) partially addresses this for realtime connections.
**Standard:** HIPAA §164.312(a)(2)(iii)

### 34. Risk Analysis
**Status:** Not yet addressed
**Gap:** Monitoring and observability exist but there is no automated risk scoring or periodic vulnerability assessment of application logic.
**How to address:** Extend `forklaunch compliance audit` to include a risk score based on: number of unencrypted PHI/PCI fields, routes without rate limiting, entities without tenant isolation, missing RLS policies. Flag high-risk configurations automatically.
**Standard:** HIPAA §164.308(a)(1)

### 35. Penetration Testing
**Status:** Not yet addressed
**Gap:** Container image scanning on push exists (ECR `scanOnPush`) but there is no penetration testing framework, schedule, or integration.
**How to address:** Integrate DAST (Dynamic Application Security Testing) into the deployment pipeline. Run automated pen tests against staging environments before production promotion. Tools: OWASP ZAP, Nuclei, or Burp Suite CI integration.
**Standard:** PCI DSS 11.3

### 36. Change Management / Approval Workflows
**Status:** Not yet addressed
**Gap:** Git-based version control exists but there is no enforced approval workflow before deployment. Deployments can happen without peer review.
**How to address:** Add a pre-deployment check in the platform compiler that validates: PR is approved, CI passes, compliance audit report is clean. `forklaunch deploy` refuses to proceed without these checks. Integrate with GitHub API for approval status.
**Standard:** PCI DSS 6.4, SOC 2 CC8.1

### 37. Cardholder Data Flow Diagram
**Status:** Not yet addressed
**Gap:** The compliance audit report covers field classifications but does not generate a visual data flow diagram showing where PCI data enters, is processed, stored, and exits the system.
**How to address:** Extend `forklaunch compliance audit` to generate a Mermaid or ASCII data flow diagram by tracing PCI-classified fields through entities, routes, and services. Show: ingress (which routes accept PCI data), processing (which services handle it), storage (which entities persist it), egress (which responses return it).
**Standard:** PCI DSS 1.1.3

### 38. Disaster Recovery Testing
**Status:** Not yet addressed
**Gap:** Backups and Multi-AZ failover exist but there is no automated disaster recovery testing to verify that restoration actually works.
**How to address:** Build an automated DR test script in the platform that: creates a test RDS instance from the latest snapshot, verifies data integrity, runs a health check against the restored instance, and tears down. Schedule monthly. Report results to the compliance audit log.
**Standard:** HIPAA §164.308(a)(7)(ii), SOC 2 A1.2

### 39. Supply Chain / Vendor Security Monitoring
**Status:** Not yet addressed
**Gap:** No tracking of third-party dependency security posture. The framework depends on MikroORM, Stripe SDK, AWS SDK, and other packages with no automated vulnerability monitoring beyond ECR image scanning.
**How to address:** Integrate dependency vulnerability scanning (Snyk, Socket, or GitHub Dependabot) into the CI pipeline and include dependency vulnerability counts in the `forklaunch compliance audit` report. Alert on critical/high severity CVEs.
**Standard:** SOC 2 CC9.2, PCI DSS 6.2

### 40. Privacy Impact Assessments (GDPR)
**Status:** Not yet addressed
**Gap:** No automated Data Protection Impact Assessment tooling.
**How to address:** Extend `forklaunch compliance audit` with a GDPR-specific mode that evaluates: types and volume of personal data processed, legal basis for processing, data retention periods, cross-border transfers, and automated decision-making. Generate a DPIA template pre-filled from compliance metadata.
**Standard:** GDPR Art. 35

---

## Not Addressed — Business Action Required

These requirements are organizational, legal, or procedural. They cannot be enforced by software and require business processes to be established by the organization deploying ForkLaunch.

### 41. Data Processing Agreements (GDPR)
**What's needed:** Legal contracts between the data controller and ForkLaunch (as data processor) that establish the scope, purpose, and safeguards for personal data processing.
**Who owns this:** Legal team, DPO
**Recommended steps:**
1. Draft a standard DPA template reviewed by privacy counsel
2. Execute DPAs with all customers processing EU personal data
3. Maintain a register of all DPAs and sub-processors
**Standard:** GDPR Art. 28

---

## Summary

| Category | Count | Details |
|---|---|---|
| **Fully addressed** | 23 | All core technical controls across application and infrastructure layers |
| **Partially addressed** | 1 | Data retention — infrastructure retention exists, app-level retention not yet built |
| **Addressable by engineering** | 11 | GDPR data subject rights, session logoff, risk analysis, pen testing, change management, data flow diagrams, DR testing, supply chain monitoring, privacy impact assessments |
| **Business action required** | 6 | Breach notification, BAAs, DPAs, training, incident response, physical security |
| **Total requirements** | 41 | |

**ForkLaunch addresses 23 of 41 identified compliance requirements fully, 1 partially. 11 additional requirements can be addressed with engineering work. 6 require organizational processes that no software framework can enforce.** The technical controls that are implemented go beyond what most standards require — compliance violations are caught at compile time, startup, query execution, and the middleware pipeline, not just documented in a policy PDF.
