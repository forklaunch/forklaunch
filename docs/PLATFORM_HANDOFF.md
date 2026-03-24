# Platform Team Handoff: Data Residency & Compliance CLI

## Context

The ForkLaunch framework now enforces compliance at the runtime level (audit logging, field encryption, tenant isolation, RBAC, rate limiting). The platform compiler needs to support two additional features that operate at **build-time and deploy-time**, not runtime.

---

## 1. Data Residency Enforcement

### What the framework provides
The manifest declares allowed deployment regions:

```toml
[compliance]
data_residency = ["us-east-1", "eu-west-1"]
```

The framework carries this field but does **nothing** at runtime. Residency is a deployment constraint, not a runtime check.

### What the platform compiler needs to do

1. **Read `[compliance].data_residency`** from `manifest.toml` during the deployment pipeline (before Pulumi runs).

2. **Validate the target deployment region** against the allowed list:
   ```
   Target region: us-west-2
   Allowed regions: [us-east-1, eu-west-1]
   → FAIL: "Deployment to us-west-2 violates data residency policy. Allowed regions: us-east-1, eu-west-1"
   ```

3. **Fail the deployment BEFORE any infrastructure is provisioned.** The check must happen before `pulumi up` to avoid partial state.

4. **Apply the constraint to ALL infrastructure resources** — RDS, ElastiCache, S3 buckets, Lambda functions, etc. must all be in allowed regions. Cross-region replication targets must also be validated.

5. **Surface the constraint in the deployment UI/CLI output** so the developer knows which regions are available before attempting deployment.

### Implementation guidance

- The check should be in the deployment pipeline between manifest parsing and Pulumi execution.
- The Pulumi state path already encodes region: `s3://<bucket>/applications/<appId>/environments/<env>/<region>/state/pulumi-state/`. Validate the `<region>` segment.
- For multi-region deployments with cross-region serialization, validate that ALL target regions are in the allowed list.
- The `data_residency` field is optional. If absent, no region restriction is applied (any region is allowed).

---

## 2. Manifest Schema Additions (CLI)

The following fields need to be added to the manifest TOML schema in the Rust CLI:

### `[compliance]` section (new, optional)

```toml
[compliance]
# Allowed deployment regions. Validated by the compiler at deploy time.
# If absent, any region is allowed.
data_residency = ["us-east-1", "eu-west-1"]

# Required secrets that must be present as environment variables at boot.
# The framework validates these at startup via SecretsAccessor.
secrets = ["ENCRYPTION_MASTER_KEY", "HMAC_SECRET_KEY", "JWKS_PUBLIC_KEY_URL"]
```

### `[compliance.entities.<EntityName>]` sections (new, optional)

```toml
# Field-level compliance classifications, stored by the CLI when scaffolding entities.
# Used by `forklaunch compliance audit` to generate reports.
[compliance.entities.User]
id = "none"
email = "pii"
medicalRecord = "phi"
cardNumber = "pci"
name = "none"

[compliance.entities.Organization]
id = "none"
name = "none"
taxId = "pci"
```

### Rust implementation notes

In `cli/src/core/manifest.rs`, add to the `internal_config_struct` macro's generated fields:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
$vis compliance: Option<ComplianceManifestConfig>,
```

Define the struct:

```rust
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub(crate) struct ComplianceManifestConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) data_residency: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) secrets: Vec<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) entities: HashMap<String, HashMap<String, String>>,
}
```

All fields are optional with `serde(default)` so existing manifests parse without changes.

---

## 3. Compliance Audit Report (CLI Command)

### Command: `forklaunch compliance audit`

Generates a point-in-time compliance report covering:
- All routes and their access levels (read from OpenAPI spec)
- All entity fields and their compliance classifications (read from manifest)
- Declared secrets and whether they're present
- Data residency configuration
- Rate limit configuration

### Output: JSON

```json
{
  "generatedAt": "2026-03-23T...",
  "routes": [
    { "path": "/user", "method": "GET", "access": "protected",
      "rbac": { "allowedRoles": ["admin"] } }
  ],
  "entities": [
    { "name": "User", "fields": [
      { "name": "email", "compliance": "pii", "encrypted": false },
      { "name": "medicalRecord", "compliance": "phi", "encrypted": true }
    ]}
  ],
  "secrets": { "declared": ["ENCRYPTION_MASTER_KEY"], "count": 1 },
  "dataResidency": { "allowedRegions": ["us-east-1"] }
}
```

### Implementation approach
- New subcommand in `cli/src/compliance/` (similar to `cli/src/openapi/`)
- Read OpenAPI spec from generated files (already exists via `forklaunch openapi export`)
- Read manifest `[compliance]` section
- Output to stdout (JSON) or a file (with `--output` flag)
- PDF output is a stretch goal (use a Rust PDF library like `printpdf`)

---

## 4. Encryption Key Management

The framework uses AES-256-GCM with HKDF-derived per-tenant keys. The master key is provided via environment variable (`ENCRYPTION_MASTER_KEY`).

### What the compiler already handles
- Environment variables are set before application boot
- Each AWS service manages its own encryption (RDS, ElastiCache, S3)

### What may need attention
- **Key rotation**: When the master key changes, all encrypted PHI/PCI data needs re-encryption. The framework supports versioned ciphertext (`v1:` prefix) to enable future key versions.
- **Key availability**: The framework refuses to start if `ENCRYPTION_MASTER_KEY` is missing and any entity has PHI/PCI fields. The compiler should ensure this env var is always set for services that have compliance entities.
