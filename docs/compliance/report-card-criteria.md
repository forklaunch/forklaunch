---
title: "Report Card Default Criteria"
description: "The 283 requirements every ForkLaunch report card checks on every analysis."
category: "Compliance"
---

## Overview

Every report card checks the same 283 requirements on every analysis, the same way each time. They are grouped into areas; for each area, ForkLaunch finds the relevant code by rule and asks one judge call to answer every requirement in it as **not met**, **not applicable** or **met**. An answer only counts when the judge is confident; otherwise the requirement is marked unsure for a person to look at.

A requirement that is not met becomes a finding on the app's running findings list, which is re-checked on every analysis: a finding resolves when the code is fixed and is flagged if it comes back. The reviewing committee is told what this list covers and proposes only specific problems outside it; each proposal is confirmed or rejected by the same judge before it reaches the list.

27 of the requirements apply only when a compliance framework (HIPAA, PCI DSS, SOC 2, GDPR, CCPA) applies to the app, either because you chose it or because it was detected from the app's description.

| Category | Requirements | Always on |
|---|---|---|
| [Security](#security) | 55 | 55 |
| [Compliance](#compliance) | 64 | 37 |
| [Governance](#governance) | 55 | 55 |
| [Scalability](#scalability) | 55 | 55 |
| [Observability](#observability) | 54 | 54 |

List version `2026.10-criteria.1`. This page is generated from the scorer's catalog; do not edit it by hand.

## Security

### Password storage and sign-in

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-passwords-01` | Stored passwords are hashed with argon2, bcrypt or scrypt | Passwords are stored in plain text or with a fast hash | critical | OWASP ASVS V6 Authentication; SOC 2 CC6.1 |
| `sec-passwords-02` | Repeated failed sign-ins are slowed or locked out | Sign-in allows unlimited password guessing | high | OWASP ASVS V6 Authentication |
| `sec-passwords-03` | Password reset tokens are random, single-use and expire | Password reset tokens are guessable, reusable or never expire | high | OWASP ASVS V6 Authentication |
| `sec-passwords-04` | Sign-in and reset responses do not reveal whether an account exists | Sign-in or reset responses reveal which accounts exist | medium | OWASP ASVS V6 Authentication |
| `sec-passwords-05` | A second factor is available, at least for administrators | No second factor is available, even for administrators | high | OWASP ASVS V6 Authentication; SOC 2 CC6.1 |

### Sessions and tokens

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-sessions-01` | Token signatures are verified with a fixed algorithm | Tokens are decoded without verifying their signature or algorithm | critical | OWASP ASVS V9 Self-contained Tokens |
| `sec-sessions-02` | Access tokens and sessions expire | Access tokens or sessions never expire | high | OWASP ASVS V7 Session Management; HIPAA 164.312(a)(2)(iii) |
| `sec-sessions-03` | Session cookies are HttpOnly, Secure and SameSite | Session cookies are readable by scripts or sent over plain HTTP | high | OWASP ASVS V7 Session Management |
| `sec-sessions-04` | Sign-out and password change end existing sessions | Sessions stay valid after sign-out or a password change | medium | OWASP ASVS V7 Session Management |
| `sec-sessions-05` | Refresh tokens are rotated and can be revoked | Refresh tokens are long-lived, reusable and cannot be revoked | medium | OWASP ASVS V7 Session Management |
| `sec-sessions-06` | Browser code does not keep long-lived tokens in localStorage | Auth tokens are stored in localStorage, readable by any injected script | medium | OWASP ASVS V7 Session Management |

### Authorization on every route

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-authz-01` | Every non-public route requires authentication | Some routes that handle user data can be called without signing in | critical | OWASP ASVS V8 Authorization; SOC 2 CC6.1 |
| `sec-authz-02` | Sensitive actions check the caller's role or permission on the server | Sensitive routes check only that a user is signed in, not what they may do | high | OWASP ASVS V8 Authorization; SOC 2 CC6.3 |
| `sec-authz-03` | Record access checks that the caller owns or may see the record | Records can be read or changed by id without an ownership check | critical | OWASP ASVS V8 Authorization |
| `sec-authz-04` | Identity and role come from the verified session, not the request | The caller's identity or role is read from client-supplied request fields | critical | OWASP ASVS V8 Authorization |
| `sec-authz-05` | Clients cannot set privileged fields through mass assignment | Request bodies are saved wholesale, so clients can set their own role or owner | high | OWASP ASVS V8 Authorization |
| `sec-authz-06` | Administrative routes are separated and restricted to admins | Administrative endpoints are open to any signed-in user | high | OWASP ASVS V8 Authorization; SOC 2 CC6.3 |

### Tenant isolation

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-tenancy-01` | Every query on tenant data is scoped to the caller's tenant | Some queries on tenant data are not scoped to a tenant | critical | OWASP ASVS V8 Authorization; SOC 2 CC6.1 |
| `sec-tenancy-02` | The tenant id comes from the verified session | The tenant is chosen by a client-supplied parameter | critical | OWASP ASVS V8 Authorization |
| `sec-tenancy-03` | Tenant-owned tables carry a required tenant column | Tenant-owned tables have no required tenant column | high | OWASP ASVS V8 Authorization |
| `sec-tenancy-04` | Cache keys and queues are namespaced by tenant | Cached or queued tenant data is not namespaced by tenant | high | OWASP ASVS V8 Authorization |
| `sec-tenancy-05` | Stored files are separated by tenant | Stored files are not separated or checked by tenant | high | OWASP ASVS V8 Authorization |

### Input validation

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-input-01` | Request bodies are validated against a schema on the server | Request bodies reach business logic without schema validation | high | OWASP ASVS V2 Validation and Business Logic |
| `sec-input-02` | Path and query parameters are validated and typed | Path and query parameters are used without validation | medium | OWASP ASVS V2 Validation and Business Logic |
| `sec-input-03` | Request body size is limited | The server accepts request bodies of unlimited size | medium | OWASP ASVS V2 Validation and Business Logic |
| `sec-input-04` | Strings and arrays in input have length limits | Input fields accept unbounded strings and arrays | low | OWASP ASVS V2 Validation and Business Logic |
| `sec-input-05` | Unknown fields in requests are rejected or stripped | Requests can carry undeclared fields through to the database | medium | OWASP ASVS V2 Validation and Business Logic |

### Injection

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-injection-01` | SQL queries use parameters, never string-built input | Request data is concatenated into raw SQL | critical | OWASP ASVS V1 Encoding and Sanitization |
| `sec-injection-02` | Document-store queries cannot receive operator objects from input | Document-store queries accept operator objects straight from input | high | OWASP ASVS V1 Encoding and Sanitization |
| `sec-injection-03` | No shell command is built from input | Shell commands are built from user input | critical | OWASP ASVS V1 Encoding and Sanitization |
| `sec-injection-04` | Input is never evaluated as code or used as a template | User input can be evaluated as code or as a template | critical | OWASP ASVS V1 Encoding and Sanitization |
| `sec-injection-05` | File paths built from input cannot escape their directory | File paths built from input allow ../ traversal | high | OWASP ASVS V5 File Handling |
| `sec-injection-06` | Search and sort parameters map to allowlisted columns | User-chosen sort or filter fields are inserted into queries unchecked | medium | OWASP ASVS V1 Encoding and Sanitization |

### Browser-facing protections

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-browser-01` | Untrusted content is not rendered as raw HTML | Untrusted data is rendered as raw HTML | high | OWASP ASVS V1 Encoding and Sanitization |
| `sec-browser-02` | Cookie-authenticated writes are protected against CSRF | Cookie-authenticated requests have no CSRF protection | high | OWASP ASVS V3 Web Frontend Security |
| `sec-browser-03` | CORS allows only known origins | CORS allows any origin, including with credentials | high | OWASP ASVS V3 Web Frontend Security |
| `sec-browser-04` | Security headers are set, including HSTS and a content security policy | Standard security headers (HSTS, CSP, nosniff) are not set | medium | OWASP ASVS V3 Web Frontend Security |
| `sec-browser-05` | Pages cannot be framed by other sites | Pages can be embedded in frames on other sites | low | OWASP ASVS V3 Web Frontend Security |
| `sec-browser-06` | Redirect targets from input are checked | Redirects follow any URL passed in a parameter | medium | OWASP ASVS V3 Web Frontend Security |

### Secrets and cryptography

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-secrets-crypto-01` | No secrets are hardcoded in source | Secrets are hardcoded in source code | critical | OWASP ASVS V13 Configuration; SOC 2 CC6.1 |
| `sec-secrets-crypto-02` | Environment files with real values are kept out of the repository | Environment files with real secrets are committed | high | OWASP ASVS V13 Configuration |
| `sec-secrets-crypto-03` | Security code uses modern cryptographic algorithms | Security code uses broken cryptography (MD5, SHA-1, DES or ECB) | high | OWASP ASVS V11 Cryptography |
| `sec-secrets-crypto-04` | Tokens and identifiers that must be unguessable use a secure random source | Secret tokens are generated with a predictable random function | high | OWASP ASVS V11 Cryptography |
| `sec-secrets-crypto-05` | TLS certificate checks are never turned off | TLS certificate verification is turned off for outbound connections | high | OWASP ASVS V12 Secure Communication |
| `sec-secrets-crypto-06` | Signatures and secrets are compared in constant time | Signatures or secrets are compared with ordinary equality | medium | OWASP ASVS V11 Cryptography |

### File uploads and outbound requests

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-uploads-outbound-01` | Uploads are limited to allowed file types, checked by content | Uploads accept any file type | high | OWASP ASVS V5 File Handling |
| `sec-uploads-outbound-02` | Upload size is limited | Uploads have no size limit | medium | OWASP ASVS V5 File Handling |
| `sec-uploads-outbound-03` | Uploaded files are stored under generated names outside the web root | Uploaded files keep client filenames in a publicly served folder | medium | OWASP ASVS V5 File Handling |
| `sec-uploads-outbound-04` | Server-side requests to user-supplied URLs are restricted | The server fetches arbitrary user-supplied URLs, including internal addresses | high | OWASP ASVS V1 Encoding and Sanitization |
| `sec-uploads-outbound-05` | Incoming webhooks are verified by signature | Incoming webhooks are acted on without verifying their signature | high | OWASP ASVS V4 API and Web Service |

### Abuse limits, errors and dependencies

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `sec-abuse-deps-01` | Public and expensive endpoints are rate limited | The API has no rate limiting | medium | OWASP ASVS V2 Validation and Business Logic; SOC 2 CC6.6 |
| `sec-abuse-deps-02` | Error responses do not expose stack traces or internals | Error responses expose stack traces or internal details | medium | OWASP ASVS V16 Security Logging and Error Handling |
| `sec-abuse-deps-03` | Debug modes and development tools are off in production | Debug modes or development tools are reachable in production | medium | OWASP ASVS V13 Configuration |
| `sec-abuse-deps-04` | Dependencies are pinned by a committed lockfile | Dependencies are not pinned by a committed lockfile | medium | OWASP ASVS V15 Secure Coding and Architecture; SOC 2 CC8.1 |
| `sec-abuse-deps-05` | Dependencies are checked for known vulnerabilities | Dependencies are not checked for known vulnerabilities | medium | OWASP ASVS V15 Secure Coding and Architecture; SOC 2 CC7.1 |

## Compliance

### Classifying stored data

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-classification-01` | Stored fields holding personal, health or payment data are tagged as such in code | Sensitive fields are not marked as sensitive in the data model | high | SOC 2 C1.1; GDPR Art. 30 |
| `cmp-classification-02` | Every entity field has an explicit classification, not only the obvious ones | Only some fields are classified; the rest default to unclassified | medium |  |
| `cmp-classification-03` | Classification drives at least one control automatically | Data classification is labelling only and changes no handling | medium |  |
| `cmp-classification-04` | Free-text fields that may hold sensitive data are treated as sensitive | Free-text notes fields can hold sensitive data but are treated as ordinary text | medium |  |
| `cmp-classification-05` | Uploaded files carry a classification like database fields do | Uploaded documents are stored without any sensitivity classification | medium |  |

### Encryption of sensitive data at rest

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-encryption-rest-01` | Sensitive fields are encrypted before they are written to the database | Sensitive fields are stored unencrypted in the database | high | HIPAA 164.312(a)(2)(iv); SOC 2 CC6.1; GDPR Art. 32 |
| `cmp-encryption-rest-02` | Encryption uses an authenticated modern cipher | Data is encrypted with a weak or unauthenticated cipher | high |  |
| `cmp-encryption-rest-03` | Encryption keys are not hard-coded | An encryption key is hard-coded in the source | critical | PCI DSS Req. 3 |
| `cmp-encryption-rest-04` | Database and file storage are encrypted at the storage layer | Database or file storage is not configured for storage-level encryption | medium | SOC 2 CC6.1 |
| `cmp-encryption-rest-05` | Backups and exports of sensitive data are encrypted | Backups or bulk exports of sensitive data are not encrypted | medium |  |

### Encryption of data in transit

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-transit-01` | Database connections require TLS in deployed environments | Database connections are not required to use TLS | high | HIPAA 164.312(e)(1); PCI DSS Req. 4 |
| `cmp-transit-02` | TLS certificate checks are not disabled | TLS certificate verification is turned off | high |  |
| `cmp-transit-03` | Cache and queue connections use TLS in deployed environments | Cache or queue traffic carrying data is not encrypted in transit | medium |  |
| `cmp-transit-04` | Calls to other services and vendors use HTTPS | Application data is sent to a service over plain HTTP | high | HIPAA 164.312(e)(1) |

### Audit trail of sensitive records

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-audit-trail-01` | Changes to sensitive records are written to an audit trail | Changes to sensitive records leave no audit trail | high | HIPAA 164.312(b); SOC 2 CC7.2 |
| `cmp-audit-trail-02` | Reads of sensitive records are audited too | Viewing sensitive records is not audited | medium | HIPAA 164.312(b) |
| `cmp-audit-trail-03` | Audit entries record who, what, which record and when | Audit entries are missing the actor, record or time | medium |  |
| `cmp-audit-trail-04` | The audit trail cannot be edited or deleted by the application | Audit records can be edited or deleted through the application | high | HIPAA 164.312(c)(1) |
| `cmp-audit-trail-05` | Audit writes cannot be skipped silently | An audit write can fail silently while the change still succeeds | medium |  |

### Retention and deletion

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-retention-01` | Retention periods are defined in code or configuration | No retention period is defined for sensitive data | medium | GDPR Art. 5 |
| `cmp-retention-02` | A scheduled job deletes data past its retention period | Expired data is never deleted automatically | medium |  |
| `cmp-retention-03` | Soft-deleted sensitive records are eventually hard-deleted | Soft-deleted records keep their sensitive data forever | medium | GDPR Art. 17 |
| `cmp-retention-04` | Deleting a record also deletes its files and derived copies | Deleting a record leaves its files or cached copies behind | medium |  |
| `cmp-retention-05` | Backup retention is bounded | Backups are kept with no retention limit | low |  |

### Keeping sensitive data out of responses, URLs and logs

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-exposure-01` | API responses return only the fields a caller needs | API responses return whole records, including fields the caller does not need | medium | HIPAA 164.502(b); GDPR Art. 25 |
| `cmp-exposure-02` | Sensitive values are not placed in URLs | Sensitive data is sent in URLs, where it ends up in logs and browser history | high |  |
| `cmp-exposure-03` | Sensitive fields are redacted before logging | Sensitive data is written to logs unredacted | high | HIPAA 164.312(b); PCI DSS Req. 10 |
| `cmp-exposure-04` | Error responses do not echo sensitive data or internals | Error responses expose record data or internal details | medium |  |
| `cmp-exposure-05` | Sensitive data is not sent to analytics or telemetry | Sensitive data is sent to analytics or error-tracking tools | high |  |

### Access, export and consent

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-subject-rights-01` | A person's data can be exported in a machine-readable format | There is no way to export a person's data | medium | GDPR Art. 15 |
| `cmp-subject-rights-02` | Exports are limited to the requester and recorded | Data exports are not limited to the requester or not recorded | medium |  |
| `cmp-subject-rights-03` | Consent and communication preferences are stored with time and source | Consent is not stored, or is stored without when and how it was given | medium | GDPR Art. 7 |
| `cmp-subject-rights-04` | Optional processing checks consent before it runs | Optional communications are sent without checking consent | medium |  |

### Third-party sharing and data location

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-sharing-01` | Third parties that receive personal data are listed in one place | There is no single list of which third parties receive personal data | medium | GDPR Art. 30; SOC 2 CC6.1 |
| `cmp-sharing-02` | Outgoing webhooks send only the fields the receiver needs | Webhooks or integrations send whole records with sensitive fields | medium | GDPR Art. 5 |
| `cmp-sharing-03` | Outgoing webhooks are signed | Outgoing webhooks are not signed | low |  |
| `cmp-sharing-04` | Where data is stored and processed is set explicitly | The region where data is stored is not set explicitly | low | GDPR Art. 44 |

### HIPAA safeguards for health data

*Only when HIPAA applies.*

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-hipaa-01` | Every field holding health information is encrypted at rest | Some health information is stored unencrypted | critical | HIPAA 164.312(a)(2)(iv) |
| `cmp-hipaa-02` | Health information is only sent to approved vendors | Health information can be sent to vendors that are not on an approved list | high | HIPAA 164.308 |
| `cmp-hipaa-03` | Access to health records is logged per record | Access to individual health records is not logged | high | HIPAA 164.312(b) |
| `cmp-hipaa-04` | Endpoints return only the health fields a role needs | Every authorised caller receives all health fields, regardless of need | medium | HIPAA 164.502(b) |
| `cmp-hipaa-05` | Idle sessions end automatically | Sessions with access to health data never time out | medium | HIPAA 164.312(a)(2)(iii) |
| `cmp-hipaa-06` | Health records are protected against undetected change | Health records can be changed without any record of the previous value | medium | HIPAA 164.312(c)(1) |
| `cmp-hipaa-07` | Health information is kept out of notifications | Notifications include health details | high |  |
| `cmp-hipaa-08` | Health data is not used in non-production environments | Real health data appears in seeds, fixtures or non-production data | high |  |

### PCI-DSS handling of card data

*Only when PCI-DSS applies.*

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-pci-01` | Full card numbers are never stored | Full card numbers are stored | critical | PCI DSS Req. 3 |
| `cmp-pci-02` | Card security codes are never stored | Card security codes are stored or logged | critical | PCI DSS Req. 3 |
| `cmp-pci-03` | Card details are collected by the payment provider, not the app server | Raw card numbers pass through the app's own servers | high | PCI DSS Req. 4 |
| `cmp-pci-04` | Card data is kept out of logs | Card data can appear in logs or error reports | critical | PCI DSS Req. 10 |
| `cmp-pci-05` | Payment keys can be rotated without a code change | Payment or encryption keys cannot be rotated without changing code | medium | PCI DSS Req. 3 |
| `cmp-pci-06` | Payment services are isolated from the rest of the network | Payment components share an open network with everything else | medium | PCI DSS Req. 1 |

### SOC 2 confidentiality and access evidence

*Only when SOC 2 applies.*

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-soc2-01` | Role and permission changes are recorded | Role and permission changes are not recorded | medium | SOC 2 CC6.2 |
| `cmp-soc2-02` | Access can be revoked immediately | Removing a user does not end their existing sessions | high | SOC 2 CC6.2 |
| `cmp-soc2-03` | Access can be listed for periodic review | There is no way to list who has which access for review | low | SOC 2 CC6.3 |
| `cmp-soc2-04` | Confidential business data is identified and protected | Confidential business data is not identified or protected | low | SOC 2 C1.1 |
| `cmp-soc2-05` | Confidential data is disposed of when no longer needed | Confidential data is kept after the account or contract ends | low | SOC 2 C1.2 |

### GDPR data-subject rights

*Only when GDPR applies.*

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-gdpr-01` | A person can have their data erased across the app | Erasing a person leaves their data in other tables or services | high | GDPR Art. 17 |
| `cmp-gdpr-02` | Personal data can be exported in a portable format | Personal data cannot be exported in a portable format | medium | GDPR Art. 20 |
| `cmp-gdpr-03` | Consent can be withdrawn as easily as given | Consent cannot be withdrawn, or withdrawing it changes nothing | medium | GDPR Art. 7 |
| `cmp-gdpr-04` | Personal data can be corrected | There is no way to correct personal data | low | GDPR Art. 16 |
| `cmp-gdpr-05` | Personal data stays in the configured region | Personal data can be stored or processed outside the configured region | medium | GDPR Art. 44 |

### CCPA consumer rights

*Only when CCPA applies.*

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `cmp-ccpa-01` | Consumers can opt out of the sale or sharing of their data | There is no working opt-out of sale or sharing | medium | CCPA 1798.120 |
| `cmp-ccpa-02` | Consumers can see what personal data is held about them | Consumers cannot get a copy of the data held about them | medium | CCPA 1798.100 |
| `cmp-ccpa-03` | Consumers can have their personal data deleted | Consumers cannot have their personal data deleted | medium | CCPA 1798.105 |

## Governance

### Continuous integration

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-ci-01` | A CI pipeline runs automatically on every pull request | No CI pipeline runs on pull requests | high | SOC 2 CC8.1 |
| `gov-ci-02` | CI builds the application | CI never builds the application | high | SOC 2 CC8.1 |
| `gov-ci-03` | CI runs the automated tests | CI does not run the tests | high | SOC 2 CC8.1 |
| `gov-ci-04` | CI type-checks and lints the code | CI runs no type check or linter | medium |  |
| `gov-ci-05` | A failing step fails the pipeline | CI build, test or lint failures are ignored | high | SOC 2 CC8.1 |
| `gov-ci-06` | CI checks that database migrations match the schema | CI does not check that migrations match the schema | medium |  |
| `gov-ci-07` | CI runs on the main branch as well as on pull requests | CI does not run on the main branch after merge | low |  |

### Change control in the repository

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-change-01` | Code owners are defined for the repository | No CODEOWNERS file assigns reviewers to code | medium | SOC 2 CC8.1 |
| `gov-change-02` | Pull requests follow a template that asks for testing and risk | No pull request template asks how a change was tested | low |  |
| `gov-change-03` | Branch protection is declared as code | Branch protection is not declared in code | medium | SOC 2 CC8.1 |
| `gov-change-04` | Changes are recorded in a changelog or release notes | Changes are not recorded in a changelog | low |  |
| `gov-change-05` | Commit messages follow an enforced convention | Commit messages follow no enforced convention | low |  |

### Reproducible builds

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-builds-01` | A dependency lockfile is committed | No dependency lockfile is committed | high |  |
| `gov-builds-02` | CI installs exactly what the lockfile says | CI may install different dependency versions than the lockfile | medium |  |
| `gov-builds-03` | Docker base images are pinned to a version | Container images use the `latest` tag or no tag | medium |  |
| `gov-builds-04` | The runtime and package manager versions are pinned | Runtime and package manager versions are not pinned | low |  |
| `gov-builds-05` | No dependency uses a wildcard or `latest` version range | Some dependencies use `*`, `latest` or an unpinned git branch | medium |  |

### Dependency hygiene

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-deps-01` | Dependency updates are automated | Dependency updates are not automated | medium |  |
| `gov-deps-02` | Each shared library is on one version across packages | Packages depend on conflicting versions of the same library | high |  |
| `gov-deps-03` | CI checks dependency alignment or known vulnerabilities | CI does not check dependencies | medium |  |
| `gov-deps-04` | Development tools are not production dependencies | Build and test tools are shipped as production dependencies | low |  |

### Environment separation and configuration

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-config-01` | Configuration is validated when the app starts | Missing configuration is not caught at startup | medium |  |
| `gov-config-02` | Environments are configured separately | All environments share one configuration | medium | SOC 2 CC8.1 |
| `gov-config-03` | Real environment files are not committed | Real environment files are committed to the repo | high |  |
| `gov-config-04` | Production values do not appear in development configuration | Development configuration points at production resources | high |  |
| `gov-config-05` | Production secrets come from a secret store or the environment | Production secrets are read from files in the repo or image | high |  |
| `gov-config-06` | An example environment file documents every setting | No example environment file lists the required settings | low |  |
| `gov-config-07` | Debug and development-only behaviour is off in production | Development-only behaviour can run in production | high |  |

### Infrastructure as code

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-iac-01` | Infrastructure is defined in code | Infrastructure is not defined in code | high | SOC 2 CC8.1 |
| `gov-iac-02` | The app runs locally from one committed definition | There is no committed way to run the app and its dependencies locally | low |  |
| `gov-iac-03` | Infrastructure state is stored remotely | Infrastructure state is local or committed to the repo | medium |  |
| `gov-iac-04` | Infrastructure changes are planned in CI before they apply | Infrastructure changes are not previewed before apply | medium | SOC 2 CC8.1 |
| `gov-iac-06` | Environments are built from the same code with different inputs | Environments use separately copied infrastructure definitions | low |  |

### Database migrations

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-migrations-01` | Schema changes are made through versioned migrations | Schema changes are not captured in versioned migrations | high | SOC 2 CC8.1 |
| `gov-migrations-02` | Production never auto-syncs the schema | The schema is auto-synced on startup, bypassing migrations | high |  |
| `gov-migrations-03` | Migrations can be rolled back | Migrations cannot be rolled back | medium |  |
| `gov-migrations-04` | Migrations run as an explicit deploy step | Migrations run implicitly on every app start | medium |  |
| `gov-migrations-05` | Committed migrations are not edited after the fact | Existing migrations appear to have been rewritten | low |  |

### Release and rollback

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-release-01` | Deployments are automated | Deployment is manual and undefined in the repo | high | SOC 2 CC8.1 |
| `gov-release-02` | Release artifacts are versioned | Release artifacts are not versioned | medium |  |
| `gov-release-03` | Production deploys need an approval or protected environment | Production deploys need no approval | medium | SOC 2 CC8.1 |
| `gov-release-04` | Deploys wait for health checks before taking traffic | Deploys do not wait for a health check | medium |  |
| `gov-release-05` | There is a defined way to roll back | No rollback path is defined | medium |  |
| `gov-release-06` | Risky features can be turned off without a deploy | Features cannot be turned off without a deploy | low |  |
| `gov-release-07` | Staging is deployed before production | Changes go straight to production with no staging step | medium |  |

### Documentation and ownership

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-docs-01` | The README says how to install, run and test the app | The README does not explain how to run and test the app | low |  |
| `gov-docs-02` | The architecture is documented | The architecture is not documented | low | SOC 2 CC2.1 |
| `gov-docs-03` | Significant design decisions are recorded | Design decisions are not recorded | low |  |
| `gov-docs-04` | Operational runbooks exist | There are no runbooks for deploy, rollback or failures | medium |  |
| `gov-docs-05` | Each service has a named owner | Services have no named owner | medium | SOC 2 CC1.3 |

### Test coverage of critical paths

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `gov-tests-01` | The app has automated tests | The app has no automated tests | high |  |
| `gov-tests-02` | Authorization rules are tested | No test checks that unauthorized access is refused | high |  |
| `gov-tests-03` | Tests exercise the real database or service boundaries | No tests run against a real database or HTTP boundary | medium |  |
| `gov-tests-04` | Coverage is measured | Test coverage is not measured | low |  |
| `gov-tests-05` | Tests are not skipped wholesale | Core tests are skipped | medium |  |

## Scalability

### Running more than one instance

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-stateless-01` | Sessions live in a shared store or in signed tokens, not in process memory | Sessions are kept in process memory, so users are signed out or split across instances | high |  |
| `scl-stateless-02` | User-uploaded and generated files are written to object storage, not the local disk | Uploaded or generated files are written to the local disk of one instance | high |  |
| `scl-stateless-03` | Data other instances need is not kept in module-level variables | Shared application state is held in in-process variables that other instances cannot see | high |  |
| `scl-stateless-04` | Locks that guard shared work are distributed, not in-process | Mutual exclusion relies on an in-process lock that does not hold across instances | medium |  |
| `scl-stateless-05` | Real-time connections work across instances | Real-time messages only reach clients connected to the same instance | medium |  |

### Paginated and bounded list endpoints

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-pagination-01` | List endpoints return one page at a time | List endpoints load every matching row in a single response | high |  |
| `scl-pagination-02` | Page size has a server-side maximum | Clients can request any page size, including all rows | medium |  |
| `scl-pagination-03` | Large or fast-changing collections use cursor (keyset) pagination | Large collections are paginated with deep OFFSET scans that slow down as data grows | low |  |
| `scl-pagination-04` | Paginated queries have a stable sort order | Paginated queries have no stable order, so pages can skip or repeat rows | low |  |
| `scl-pagination-05` | Internal queries that fetch collections are bounded too | Internal code loads whole tables into memory | medium |  |

### Indexes and query efficiency

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-queries-01` | Foreign-key columns are indexed | Foreign-key columns have no indexes, so joins and lookups scan whole tables | medium |  |
| `scl-queries-02` | Columns used in filters and sorts are indexed | Frequently filtered or sorted columns have no supporting index | medium |  |
| `scl-queries-03` | Lookup keys that must be unique have unique constraints | Uniqueness is only checked in code, which fails under concurrent writes | medium |  |
| `scl-queries-04` | Related records are loaded in one query, not one query per row | Related records are fetched one query per row (N+1 queries) | medium |  |
| `scl-queries-05` | Queries select what they need | Queries load full rows and relations, including large fields the caller never uses | low |  |
| `scl-queries-06` | Counts and aggregates run in the database | Aggregates are computed by loading every row into the application | low |  |

### Connection pools and caching

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-connections-01` | Database connections come from a pool with an explicit maximum | Database connections have no configured pool limit, so more instances can exhaust the database | medium |  |
| `scl-connections-02` | Clients are created once and reused | Clients or connections are created per request | medium |  |
| `scl-connections-03` | Idle and acquire timeouts are set on the pool | Pool acquire has no timeout, so requests hang when the pool is exhausted | low |  |
| `scl-connections-04` | Cached entries expire | Cache entries are written without an expiry | medium |  |
| `scl-connections-05` | Writes invalidate the cache entries they make stale | Updates do not invalidate cached copies, so users see stale data until the TTL runs out | low |  |

### Background work and queues

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-background-01` | Slow or failure-prone work runs in a background worker | Slow or failure-prone work runs inline in request handlers | medium |  |
| `scl-background-02` | Queued jobs retry with backoff | Background jobs are not retried, or retry immediately without limit | medium |  |
| `scl-background-03` | Jobs that keep failing are kept for inspection | Jobs that exhaust their retries are silently dropped | medium |  |
| `scl-background-04` | Worker concurrency is configured | Workers process jobs with no concurrency limit | low |  |
| `scl-background-05` | Bulk jobs process data in bounded batches | Bulk jobs load and process every record in one go | medium |  |
| `scl-background-06` | Job payloads carry references, not large data | Queued jobs carry large payloads, bloating the queue store | low |  |

### Idempotent writes and webhooks

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-idempotency-01` | Webhook handlers ignore events they have already processed | Webhook handlers process redelivered events again | high |  |
| `scl-idempotency-02` | Retried create requests do not create duplicates | A retried create request produces a duplicate payment, order or record | high |  |
| `scl-idempotency-03` | Job handlers are safe to run twice | Job handlers repeat side effects when a message is delivered twice | medium |  |
| `scl-idempotency-04` | Outgoing calls that create things pass an idempotency key | Retried outbound create calls can create duplicate charges or resources | medium |  |
| `scl-idempotency-05` | Concurrent updates do not overwrite each other | Concurrent updates can overwrite each other (lost updates) | medium |  |

### Timeouts, retries and degradation

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-resilience-01` | Every outbound HTTP call has a timeout | Outbound HTTP calls have no timeout, so a slow dependency ties up requests | medium |  |
| `scl-resilience-02` | Database queries have a time limit | Database queries can run indefinitely | low |  |
| `scl-resilience-03` | Retries back off with jitter and stop | Failed calls are retried immediately or without limit, amplifying load on a struggling dependency | medium |  |
| `scl-resilience-04` | Only safe failures are retried | Retries repeat non-transient errors or non-idempotent writes | low |  |
| `scl-resilience-05` | A failing dependency is cut off or degraded gracefully | A failing dependency is called until it drags down every request that touches it | low |  |
| `scl-resilience-06` | Parallel fan-out is bounded | Code fires an unbounded number of parallel calls at once | low |  |

### Request size and large responses

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-payloads-01` | Request bodies have a size limit | Request bodies have no explicit size limit | medium |  |
| `scl-payloads-02` | Uploads have a maximum file size | File uploads have no size or count limit | medium |  |
| `scl-payloads-03` | Large files go straight to object storage | Large files are buffered in server memory on their way to or from storage | medium |  |
| `scl-payloads-04` | Large exports are streamed or generated in the background | Large exports are built entirely in memory during the request | medium |  |
| `scl-payloads-06` | Batch endpoints limit how many items one request may carry | Batch endpoints accept any number of items in one request | low |  |

### Capacity, autoscaling and shutdown

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-runtime-01` | Containers declare CPU and memory requests or limits | App containers have no CPU or memory sizing, so capacity and noisy neighbours are unmanaged | medium |  |
| `scl-runtime-02` | Services scale out automatically | No autoscaling is configured; capacity is fixed | medium |  |
| `scl-runtime-03` | Production runs more than one instance | Production runs a single instance, so any restart or crash is an outage | medium |  |
| `scl-runtime-04` | The app drains in-flight work on shutdown | The app exits immediately on shutdown, dropping in-flight requests and jobs | medium |  |
| `scl-runtime-05` | The platform gives the app time to drain | The platform kills containers without a grace period for draining | low |  |
| `scl-runtime-06` | Workers scale separately from the web tier | Workers run inside the web process and cannot be scaled on their own | low |  |

### Scheduled jobs, migrations and load tests

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `scl-operations-01` | Scheduled jobs run once, not once per instance | Scheduled jobs run in every instance, so they execute several times as the app scales | high |  |
| `scl-operations-02` | Long scheduled runs cannot overlap themselves | A slow scheduled job can overlap its next run | low |  |
| `scl-operations-03` | Indexes on large tables are built without locking writes | Migrations build indexes in a way that locks writes on large tables | medium |  |
| `scl-operations-04` | Schema changes stay compatible with the running version | Migrations make breaking schema changes that fail requests during a rolling deploy | medium |  |
| `scl-operations-05` | Data backfills run in batches | Backfills update whole tables in one statement, holding long locks | low |  |
| `scl-operations-06` | The app has load or performance tests | There are no load or performance tests, so capacity is unknown | low |  |

## Observability

### Structured application logging

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-logging-01` | Server code logs through one shared logger | Server code logs with scattered console.log or print calls instead of a shared logger | medium | SOC 2 CC7.2 |
| `obs-logging-02` | Logs are written as structured JSON | Logs are free-form text that log tools cannot search by field | medium |  |
| `obs-logging-03` | Log level is set per environment | The log level is hard-coded and cannot differ between environments | low |  |
| `obs-logging-04` | HTTP requests are logged with method, route, status and duration | HTTP requests are not logged with status and duration | medium | SOC 2 CC7.2 |
| `obs-logging-05` | Log calls use levels that match their severity | Every log call uses the same level, so failures cannot be filtered from noise | low |  |
| `obs-logging-06` | Startup is logged with the version | Startup logs do not say which version is running | low |  |
| `obs-logging-07` | Configuration is never logged with secrets | Startup or debug logs dump configuration that includes secrets | high |  |

### What logs and traces must not contain, and where they go

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-log-hygiene-01` | The logger redacts secrets and credentials | Logs can capture passwords, tokens or authorization headers | high | SOC 2 CC6.1 |
| `obs-log-hygiene-02` | Personal or health data is redacted from logs | Personal or health data is written to logs in clear text | high |  |
| `obs-log-hygiene-03` | Request bodies are not logged wholesale | Full request or response bodies are logged, including whatever sensitive data they carry | medium |  |
| `obs-log-hygiene-04` | Trace attributes exclude sensitive values | Traces record credentials or personal data in span attributes | medium |  |
| `obs-log-hygiene-05` | Logs are shipped to a central store | Logs stay on the host or container and are lost when it is replaced | medium | SOC 2 CC7.2 |
| `obs-log-hygiene-06` | Log retention is set explicitly | Log retention is not set, so logs are kept forever or for an unknown period | low |  |

### Request and correlation ids

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-correlation-01` | Each incoming request gets a request id | Requests have no request id, so their log lines cannot be tied together | medium |  |
| `obs-correlation-02` | The request id appears on every log line for that request | Log lines written while handling a request do not carry its request id | medium |  |
| `obs-correlation-03` | Calls to other services forward the id | Calls between services drop the request id, so one user action cannot be followed across services | medium |  |
| `obs-correlation-04` | Queued jobs carry the id of the request that created them | Background jobs lose the id of the request that created them | low |  |

### Error handling and error tracking, server and browser

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-errors-01` | Caught errors are logged or re-thrown, never silently swallowed | Errors are caught and silently discarded | high |  |
| `obs-errors-02` | Error logs include the stack and context | Error logs record only a message, without the stack or what was being done | medium |  |
| `obs-errors-03` | A central error handler turns unhandled errors into logged 500s | Unhandled errors bypass logging or return stack traces to the caller | medium |  |
| `obs-errors-04` | Process-level crashes are logged before exit | Crashes from unhandled promise rejections or exceptions leave no log | medium |  |
| `obs-errors-05` | Errors are reported to an error-tracking service | Server errors are not sent to any error-tracking service | medium | SOC 2 CC7.3 |
| `obs-errors-06` | Browser errors are reported to a tracking service | Errors in the browser are never reported, so broken screens go unnoticed | medium |  |
| `obs-errors-07` | Rendering errors are caught by an error boundary | A rendering error blanks the whole page and is not reported | low |  |
| `obs-errors-08` | Frontend error reports exclude personal data | Frontend error reports can include personal data typed by users | medium |  |

### Metrics

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-metrics-01` | Request count is recorded as a metric | Request volume is not measured | medium | SOC 2 CC7.2 |
| `obs-metrics-02` | Request errors are recorded as a metric | The error rate cannot be read from metrics | medium |  |
| `obs-metrics-03` | Request latency is recorded as a histogram | Request latency is not measured as a distribution, so slow tails are invisible | medium |  |
| `obs-metrics-04` | Key business events are counted | No business events are measured, so a silent drop in activity goes unnoticed | low |  |
| `obs-metrics-05` | Dependency calls are measured | Database and external API calls are not measured, so slow dependencies cannot be spotted | medium |  |

### Distributed tracing

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-tracing-01` | Tracing is initialised at startup | The service has no tracing | medium | SOC 2 CC7.2 |
| `obs-tracing-02` | Traces are exported to a collector or backend | Spans are created but never exported anywhere | medium |  |
| `obs-tracing-03` | Trace context crosses service boundaries | Each service starts a new trace, so a request cannot be followed across services | medium |  |
| `obs-tracing-04` | Trace context crosses queues | Background jobs appear as unrelated traces, disconnected from the request that queued them | low |  |

### Health and readiness

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-health-01` | Each service exposes a health endpoint | Services have no health endpoint for the platform to check | medium | SOC 2 CC7.2 |
| `obs-health-02` | Liveness and readiness are separate | One health check serves both liveness and readiness, so a slow dependency can get healthy instances restarted | low |  |
| `obs-health-03` | Readiness checks the service's dependencies | The readiness check reports ready even when the database or cache is unreachable | medium |  |
| `obs-health-04` | The deployment uses the health checks | Health endpoints exist but nothing in the deployment checks them | medium |  |
| `obs-health-05` | Shutdown is graceful and logged | Services stop abruptly on deploy, dropping in-flight requests without a trace | low |  |

### Background job and queue visibility

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-workers-01` | Job failures are logged with the job id and error | Background job failures are not logged with enough detail to find the job | medium |  |
| `obs-workers-02` | Job duration and outcomes are measured | Job duration and failure rate are not measured | low |  |
| `obs-workers-03` | Queue depth is visible | Queue backlog is invisible, so a stuck worker is noticed only by users | medium |  |
| `obs-workers-04` | Exhausted jobs go to a dead-letter queue or failed set | Jobs that run out of retries are dropped with no record | medium |  |
| `obs-workers-05` | Dead-lettered jobs are monitored | Nobody is told when jobs pile up in the dead-letter queue | low |  |

### Alerts, SLOs and dashboards

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-alerting-01` | Alerts are defined as code | No alerts are defined in code, so nobody is paged when the app breaks | high | SOC 2 CC7.2 |
| `obs-alerting-02` | There is an alert on the error rate | A rise in errors does not trigger any alert | medium | SOC 2 CC7.3 |
| `obs-alerting-03` | There is an alert on latency or availability | Slowness or downtime does not trigger any alert | medium |  |
| `obs-alerting-04` | Alerts are routed to a person | Alerts are defined but not routed to anyone | medium | SOC 2 CC7.3 |
| `obs-alerting-05` | Service level objectives are written down | No service level objectives exist, so there is no agreed line for "too broken" | low |  |
| `obs-alerting-06` | Dashboards are defined as code | Dashboards are not kept in code and can drift or disappear | low |  |

### Security events for detection

| ID | Requirement | Finding when not met | Severity | References |
|---|---|---|---|---|
| `obs-security-events-01` | Failed sign-ins are logged as distinct events | Failed sign-in attempts are not logged, so password guessing goes unseen | medium | SOC 2 CC7.2 |
| `obs-security-events-02` | Permission denials are logged | Permission denials are not logged, so probing for access goes unseen | medium | SOC 2 CC7.2 |
| `obs-security-events-03` | Rate-limit and invalid-token events are logged | Rejected tokens and rate-limit hits are not logged | low |  |
| `obs-security-events-04` | Security events can raise an alert | A burst of failed sign-ins or denied requests does not alert anyone | medium | SOC 2 CC7.3 |
