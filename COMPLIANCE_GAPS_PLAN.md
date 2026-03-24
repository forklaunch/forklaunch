# Plan: ForkLaunch Compliance Gaps — Phase 2

## Overview

7 compliance gaps to close, building on the existing compliance framework (fp.compliance(), compliance audit CLI, manifest metadata). All items extend existing infrastructure — no new framework primitives needed.

**Scope mode:** REDUCTION — 7 build, 4 deferred (buy or Phase 3).

## Items to Build

### 1. GDPR Right to Erasure — `forklaunch gdpr erase`

**Standard:** GDPR Art. 17

**What it does:** Cascade-deletes all PII/PHI data for a specific user across all entities.

**Implementation:**
- New Rust CLI module: `cli/src/gdpr/mod.rs`, `cli/src/gdpr/erase.rs`
- Register `forklaunch gdpr erase --user-id <ID>` command in `main.rs`
- Reads manifest `[compliance.entities]` to identify all entities with PII/PHI fields
- For each entity: generates a MikroORM query that finds records linked to the user ID and deletes them
- Execution: connects to the database directly (same as migration commands) or generates a TypeScript script that the user runs
- Outputs: list of entities affected, record counts deleted, audit log entry

**Approach decision:** Generate a TypeScript script (not execute SQL directly) because the framework's tenant isolation and encryption layers need to be active during deletion. The script imports the entity schemas, connects via MikroORM, and cascades.

**Files:**
- `cli/src/gdpr/mod.rs` — module with `erase` and `export` subcommands
- `cli/src/gdpr/erase.rs` — generates erasure script
- `cli/src/main.rs` — register `gdpr` command
- Template: `cli/src/templates/gdpr/erase.ts.template` — TypeScript erasure script template

**Test:** Shell test `cli/tests/gdpr_erase.sh` — init an app, generate erasure script, verify it compiles.

---

### 2. GDPR Data Portability — `forklaunch gdpr export`

**Standard:** GDPR Art. 20

**What it does:** Exports all personal data for a specific user as JSON.

**Implementation:**
- `cli/src/gdpr/export.rs` — generates export script
- Same approach as erasure: generates a TypeScript script that connects via MikroORM, queries all entities with PII/PHI fields linked to the user, collects results, outputs JSON
- Output includes: entity name, field names, field values, compliance classification per field
- Decrypts PHI/PCI fields (using the FieldEncryptor) so the export contains plaintext

**Files:**
- `cli/src/gdpr/export.rs`
- Template: `cli/src/templates/gdpr/export.ts.template`

**Test:** Shell test `cli/tests/gdpr_export.sh`

---

### 3. Change Management — CLI generates CI + branch protection

**Standard:** PCI DSS 6.4, SOC 2 CC8.1

**What it does:** When `forklaunch init application` scaffolds a new project, it generates:
- `.github/workflows/ci.yml` — runs lint, type-check, and tests on PRs
- `.github/branch-protection.md` — instructions for enabling branch protection (can't be done via file — requires GitHub API or UI)
- Optionally: a `forklaunch compliance gate` pre-deploy check that verifies CI passed

**Implementation:**
- Add CI workflow template: `cli/src/templates/github/ci.yml`
- Add branch protection guide: `cli/src/templates/github/BRANCH_PROTECTION.md`
- Generate during `forklaunch init application`
- The CI workflow runs: `pnpm lint`, `pnpm build` (tsgo --noEmit or tsgo -b), `pnpm test`

**Files:**
- `cli/src/templates/github/ci.yml` — Mustache template with runtime/framework conditionals
- `cli/src/templates/github/BRANCH_PROTECTION.md`
- `cli/src/init/application.rs` — add GitHub template generation

**Test:** Existing `init_service.sh` test should pick up the new generated files.

---

### 4. Supply Chain Monitoring — CLI generates Dependabot

**Standard:** SOC 2 CC9.2, PCI DSS 6.2

**What it does:** Generates `.github/dependabot.yml` in the target repo during `forklaunch init application`.

**Implementation:**
- Add template: `cli/src/templates/github/dependabot.yml`
- Template conditionals: npm for Node projects, bun for Bun projects, GitHub Actions always
- Warn by default — Dependabot opens PRs but doesn't block anything
- Generate during `forklaunch init application`

**Files:**
- `cli/src/templates/github/dependabot.yml`
- `cli/src/init/application.rs` — add Dependabot template generation

**Test:** Existing `init_service.sh` should generate the file.

---

### 5. Cardholder Data Flow Diagram — extend `compliance audit`

**Standard:** PCI DSS 1.1.3

**What it does:** `forklaunch compliance audit --data-flow` generates a Mermaid diagram showing where PCI-classified data flows through the system.

**Implementation:**
- Extend `cli/src/compliance/audit.rs`
- Read OpenAPI spec for routes that accept/return PCI fields
- Read manifest `[compliance.entities]` for PCI-classified fields
- Generate Mermaid flowchart: `Route (POST /payment) --> Service --> Entity.cardNumber [PCI] --> Database`
- Output as `.md` file with Mermaid code block, or as Mermaid CLI SVG

**Data sources (already available):**
- OpenAPI spec: routes, request/response schemas
- Manifest: entity field classifications
- Route access levels: from OpenAPI `x-access` extension

**Files:**
- `cli/src/compliance/audit.rs` — add `--data-flow` flag and diagram generation

**Test:** Generate for a project with PCI fields, verify Mermaid syntax is valid.

---

### 6. Risk Analysis / Scoring — extend `compliance audit`

**Standard:** HIPAA §164.308(a)(1)

**What it does:** `forklaunch compliance audit --risk-score` adds a risk assessment section to the audit report.

**Scoring criteria:**
- Unencrypted PHI/PCI fields (high risk): `compliance: 'phi'|'pci'` without encryption → 10 points each
- Routes without rate limiting (medium): no rate limit config → 5 points each
- Entities without tenant isolation (high): entity has no `organizationId` field → 10 points each
- Missing RLS policies (medium): PostgreSQL without RLS enabled → 5 points
- Routes without access control (critical): `access: 'public'` with PII in response → 15 points each
- Missing secrets at boot (critical): undeclared secrets → 15 points each

**Output:** Risk score (0-100), risk level (Low/Medium/High/Critical), itemized findings.

**Files:**
- `cli/src/compliance/audit.rs` — add `--risk-score` flag and scoring logic
- Add `riskScore` field to the JSON report output

**Test:** Generate for test project, verify score calculation.

---

### 7. Privacy Impact Assessment — extend `compliance audit`

**Standard:** GDPR Art. 35

**What it does:** `forklaunch compliance audit --dpia` generates a DPIA template pre-filled from compliance metadata.

**Output (JSON + human-readable):**
- Data processing inventory: which entities process personal data, what types (PII/PHI/PCI), how many fields
- Legal basis: prompted or inferred (consent, contract, legal obligation)
- Data residency: from manifest `[compliance.data_residency]`
- Retention periods: from manifest (when data retention is implemented) or "not configured"
- Cross-border transfers: detected from data residency config
- Risk assessment: from risk scoring (item 6)
- Mitigations: auto-populated from existing controls (encryption, tenant isolation, audit logging, access control)

**Files:**
- `cli/src/compliance/audit.rs` — add `--dpia` flag
- Template: `cli/src/templates/compliance/dpia.json.template`

**Test:** Generate for test project with PII fields, verify DPIA structure.

---

## Implementation Order

```
Step 1: GDPR module (erase + export)         [CLI: Rust]     depends on: nothing
Step 2: GitHub templates (CI + Dependabot)    [CLI: Rust]     depends on: nothing
Step 3: Data flow diagram                     [CLI: Rust]     depends on: nothing
Step 4: Risk scoring                          [CLI: Rust]     depends on: nothing
Step 5: DPIA                                  [CLI: Rust]     depends on: Step 4 (risk score)
```

Steps 1-4 are independent — can be parallelized.

---

## NOT in Scope (Deferred)

| Item | Reason | When |
|------|--------|------|
| Data Retention / Disposal | Needs new framework primitive (`fp.compliance().retention()`), scheduled job infrastructure, migration tooling | Phase 3 |
| Consent Management | Buy OneTrust/Osano. If building: needs consent entity, middleware changes, UI components | Phase 3 or buy |
| Penetration Testing | Buy HackerOne/Cobalt for managed pen tests. OWASP ZAP for CI DAST | Buy |
| DR Testing | Buy AWS Resilience Hub. Or build monthly restore-and-verify script | Phase 3 or buy |

---

## Summary

| Category | Count |
|----------|-------|
| Items to build | 7 |
| All Rust CLI changes | Yes — no framework changes needed |
| Deferred | 4 (buy or Phase 3) |
| Dependencies on existing code | compliance metadata, manifest, OpenAPI spec, audit CLI |
| New framework primitives | None |
| Estimated complexity | Small-Medium per item |
