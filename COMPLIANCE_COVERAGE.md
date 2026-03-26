# ForkLaunch Compliance Coverage Report

*Updated: March 24, 2026*

ForkLaunch addresses 34 of 43 identified compliance requirements across HIPAA, SOC 2 (all five trust service criteria: Security, Availability, Processing Integrity, Confidentiality, and Privacy), PCI DSS, and GDPR. Controls are enforced across three layers: the **Framework** (`@forklaunch/core` — compile-time types, runtime middleware, encryption, tenant isolation), the **CLI** (`forklaunch` binary — code generation, compliance auditing, GitHub configs), and the **Platform** (ForkLaunch Cloud — risk scoring, data flow diagrams, DPIA, portal dashboard, deploy-time enforcement). 3 remaining gaps are addressable by engineering. 6 require organizational processes.

For the full interactive report with layer tags, see `compliance-coverage.html`.

---

## Summary

| Category | Count | Details |
|---|---|---|
| **Fully addressed** | 34 | All core technical controls including SOC 2 Processing Integrity + Privacy |
| **Addressable by engineering** | 3 | Consent management, penetration testing, disaster recovery testing |
| **Business action required** | 6 | Breach notification, BAAs, DPAs, training, incident response, physical security |
| **Total requirements** | 43 | |

---

## Technical Controls — Fully Addressed (32)

| # | Requirement | Layer | Standards |
|---|-------------|-------|-----------|
| 1 | Field-Level Data Classification | Framework | HIPAA §164.312(a), PCI DSS 3.1, SOC 2 CC6.1 |
| 2 | PHI/PCI Application-Level Encryption | Framework | HIPAA §164.312(a)(2)(iv), PCI DSS 3.4 |
| 3 | Encryption at Rest (Infrastructure) | Platform | HIPAA §164.312(a)(2)(iv), PCI DSS 3.4, SOC 2 CC6.1 |
| 4 | Encryption in Transit | Platform | HIPAA §164.312(e)(1), PCI DSS 4.1, SOC 2 CC6.1 |
| 5 | Tenant Isolation (Application) | Framework | HIPAA §164.312(a)(1), SOC 2 CC6.1/CC6.3 |
| 6 | Tenant Isolation (Infrastructure) | Platform | PCI DSS 1.3, SOC 2 CC6.1 |
| 7 | Access Control (Application) | Framework | HIPAA §164.312(d), PCI DSS 7.1, SOC 2 CC6.1 |
| 8 | Access Control (Infrastructure) | Platform | HIPAA §164.312(d), PCI DSS 7.2, SOC 2 CC6.2/CC6.3 |
| 9 | Audit Logging (Application) | Framework | HIPAA §164.312(b), PCI DSS 10.1, SOC 2 CC7.2 |
| 10 | Audit Logging (Infrastructure) | Platform | HIPAA §164.312(b), PCI DSS 10.2, SOC 2 CC7.2 |
| 11 | Rate Limiting | Framework | PCI DSS 6.5.10, SOC 2 CC6.1 |
| 12 | Secrets Management | Framework | HIPAA §164.312(a)(2)(iv), PCI DSS 3.5, SOC 2 CC6.1 |
| 13 | Key Rotation | Framework + Platform | PCI DSS 3.6, SOC 2 CC6.1 |
| 14 | Network Segmentation | Platform | PCI DSS 1.1/1.3, SOC 2 CC6.1 |
| 15 | Vulnerability Scanning | Platform | PCI DSS 6.1, SOC 2 CC7.1 |
| 16 | Container Security | Platform | PCI DSS 6.5, SOC 2 CC6.1 |
| 17 | Multi-Region / Data Residency | Platform | GDPR Art. 44-49, SOC 2 CC6.1 |
| 18 | Monitoring / Observability | Framework + Platform | HIPAA §164.308(a)(1), SOC 2 CC7.2/CC7.3 |
| 19 | WebSocket Security | Framework | SOC 2 CC6.1 |
| 20 | Compliance Reporting | CLI + Platform | HIPAA §164.308(a)(8), SOC 2 CC4.1, PCI DSS 12.2 |
| 21 | Backup / Recovery | Platform | HIPAA §164.308(a)(7), SOC 2 A1.2 |
| 22 | Certificate Management | Platform | PCI DSS 4.1, SOC 2 CC6.1 |
| 23 | Password / Credential Policies | Framework + Platform | PCI DSS 8.2, SOC 2 CC6.1 |
| 24 | Automatic Session Logoff | Framework | HIPAA §164.312(a)(2)(iii) |
| 25 | Right to Erasure | Framework | GDPR Art. 17 |
| 26 | Data Portability | Framework | GDPR Art. 20 |
| 27 | Risk Analysis | Platform | HIPAA §164.308(a)(1) |
| 28 | Change Management | CLI | PCI DSS 6.4, SOC 2 CC8.1 |
| 29 | Cardholder Data Flow Diagram | Platform | PCI DSS 1.1.3 |
| 30 | Supply Chain Monitoring | CLI | SOC 2 CC9.2, PCI DSS 6.2 |
| 31 | Privacy Impact Assessments | Platform | GDPR Art. 35 |
| 32 | Data Retention / Disposal | Framework + CLI + Platform | HIPAA §164.530(j), PCI DSS 3.1, SOC 2 CC6.5, SOC 2 P4.1 |
| 33 | Processing Integrity | Framework + CLI | SOC 2 PI1.1-PI1.5 |
| 34 | Privacy Controls | Framework + CLI + Platform | SOC 2 P1.1, P3.1, P4.1, P5.1, P6.1, P7.1, GDPR Art. 5(1) |

---

## Known Gaps — Addressable by Engineering (3)

| # | Requirement | Target Layer | Standards |
|---|-------------|-------------|-----------|
| 35 | Consent Management | Framework | GDPR Art. 7 |
| 36 | Penetration Testing | Platform | PCI DSS 11.3 |
| 37 | Disaster Recovery Testing | Platform | HIPAA §164.308(a)(7)(ii), SOC 2 A1.2 |

---

## Business Action Required (6)

| # | Requirement | Owner | Standards |
|---|-------------|-------|-----------|
| 38 | Breach Notification | Legal, CISO | HIPAA §164.408, GDPR Art. 33-34 |
| 39 | Business Associate Agreements | Legal, Sales | HIPAA §164.308(b), §164.502(e) |
| 40 | Data Processing Agreements | Legal, DPO | GDPR Art. 28 |
| 41 | Employee Security Training | HR, CISO | HIPAA §164.308(a)(5), PCI DSS 12.6, SOC 2 CC1.4 |
| 42 | Incident Response Plan | CISO, Engineering | PCI DSS 12.10, SOC 2 CC7.3/CC7.4, HIPAA §164.308(a)(6) |
| 43 | Physical Security | AWS (shared responsibility) | PCI DSS 9.x, SOC 2 CC6.4 |
