//! Local compliance checks that gate proper setup, run entirely offline.
//!
//! Two check families:
//! 1. Wiring checks — every project that owns persistence entities must
//!    register the tenant-isolation abstractions (`setupTenantFilter`,
//!    `setupRls`) in each runtime entrypoint, and register the retention /
//!    erasure services when its entities declare data that needs them.
//! 2. Sensitive-field heuristics — fields whose names look like PII / PHI /
//!    PCI but are classified `none` (or belong to entities that skip
//!    compliance annotation entirely) are surfaced for review.

use std::{fs, path::Path};

use anyhow::Result;
use serde::Serialize;

use crate::core::ast::infrastructure::compliance::scan_entity_compliance;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    Warning,
    Info,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalFinding {
    pub(crate) severity: Severity,
    pub(crate) project: String,
    pub(crate) check: String,
    pub(crate) subject: String,
    pub(crate) message: String,
}

/// Entrypoint files that host a runtime and therefore must wire tenant
/// isolation when the project owns a database.
const ENTRYPOINTS: &[&str] = &["server.ts", "worker.ts"];

const TENANT_GATES: &[(&str, &str)] = &[
    (
        "setupTenantFilter(",
        "does not call setupTenantFilter — queries in this runtime are not tenant-filtered",
    ),
    (
        "setupRls(",
        "does not call setupRls — PostgreSQL row-level security is not enforced in this runtime",
    ),
];

/// Better Auth handles `/api/auth/*` itself and reads the database through
/// whatever ORM it was handed at construction. If that ORM is the raw one while
/// the project's own EntityManager goes through `wrapEmWithTenantContext`, the
/// two sides derive different encryption keys: `FieldEncryptor` derives per
/// tenant via HKDF, and Better Auth reads with no tenant at all.
///
/// The rows are written under the tenant key and read back under the empty one,
/// which fails with "ciphertext is corrupted or the wrong key was used" — as a
/// 500 from sign-in or sign-up, and with nothing in the logs, because Better
/// Auth swallows its own errors. It is invisible in a single-tenant deployment,
/// where both sides agree on the empty context, and appears the first time a
/// tenant id is supplied.
const BETTER_AUTH_CONFIG_MARKERS: &[&str] = &["betterAuthConfig(", "betterAuth("];
const ENCRYPTION_AWARE_ORM_MARKERS: &[&str] = &[
    "createEncryptionAwareOrm(",
    "createTenantAwareBetterAuthOrmProxy(",
];

/// Files that can legitimately hold either half of the Better Auth wiring.
/// The blueprint wraps the ORM at the registration site; forklaunch-platform
/// wraps it inside `auth.ts` instead. Reading only one of the two would flag a
/// correctly-wired service, so both are searched together.
const BETTER_AUTH_WIRING_FILES: &[&str] = &["registrations.ts", "auth.ts"];

/// True when the project wires Better Auth against an ORM that was not made
/// encryption-aware. Returns false when Better Auth is not used at all.
fn better_auth_orm_is_unwrapped(sources: &str) -> bool {
    let wires_better_auth = BETTER_AUTH_CONFIG_MARKERS
        .iter()
        .any(|marker| sources.contains(marker));
    if !wires_better_auth {
        return false;
    }

    !ENCRYPTION_AWARE_ORM_MARKERS
        .iter()
        .any(|marker| sources.contains(marker))
}

/// Directories that never hold production wiring. Tests in particular must be
/// excluded: a project whose only `withEncryptionContext` call is in a test is
/// not wired, and counting it would hide exactly the gap being looked for.
const NON_PRODUCTION_DIRS: &[&str] = &["node_modules", "dist", "__test__", "migrations"];

/// Concatenates the project's production TypeScript. The wiring these checks
/// look for is not confined to one file — forklaunch-platform registers the
/// encryptor in `mikro-orm.config.ts` and binds tenants from `auth.ts` and its
/// services, while the blueprints do both from `registrations.ts`. Reading a
/// fixed filename flags correctly-wired services, which is a false positive
/// this check has already produced once.
fn read_production_sources(project_path: &Path) -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !NON_PRODUCTION_DIRS.contains(&name.as_str()) {
                    walk(&path, out);
                }
            } else if name.ends_with(".ts") && !name.ends_with(".test.ts") {
                if let Ok(text) = fs::read_to_string(&path) {
                    out.push_str(&text);
                    out.push('\n');
                }
            }
        }
    }

    let mut out = String::new();
    walk(project_path, &mut out);
    out
}

/// Any of these means the service can bind an encryption tenant. There is more
/// than one legitimate way: the blueprints wrap the EntityManager, while
/// forklaunch-platform's IAM enters the context directly around its reads.
const TENANT_BINDING_MARKERS: &[&str] = &[
    "wrapEmWithTenantContext(",
    "withEncryptionContext(",
    "setEncryptionTenantId(",
];

/// An EntityManager factory that sets the tenant QUERY FILTER but never the
/// tenant ENCRYPTION context.
///
/// `wrapEmWithTenantContext(em, tenantId)` does three things: sets the filter
/// params, sets the encryption tenant, and wraps the EM so the context survives
/// the pg connection pool. A factory that only calls `setFilterParams('tenant',
/// ..)` does the first and skips the rest, so every call site that passes a
/// tenant id gets row filtering, reasonably believes it is tenant-scoped, and
/// still decrypts with the no-tenant key.
///
/// Not hypothetical: it took down sign-up, the onboarding /me call and an
/// invitation lookup in one week on a service whose factory had drifted this
/// way, while the generated blueprints — which all use the helper — were fine.
const TENANT_FILTER_ONLY_MARKER: &str = "setFilterParams('tenant'";
const TENANT_CONTEXT_HELPER: &str = "wrapEmWithTenantContext(";

/// Where the encryptor is registered. Without it `EncryptedType` does not fail
/// loudly on write — it falls through to `this.serialize(value)` and stores
/// PLAINTEXT in a column declared `pii`/`pci`/`phi`. The read side does throw
/// ("no encryptor registered but database contains encrypted value"), but only
/// once a previously-encrypted row is read back, which may be much later.
/// Silent plaintext at rest is the worse half, so this is a warning.
/// Where the EntityManager is bound to a tenant. Unlike the encryptor this is
/// not required for a single-tenant service to be correct — the helper no-ops
/// when the tenant id is `undefined`. What its absence means is that the tenant
/// capability is not wired at all, so encrypted columns can only ever use the
/// no-tenant key. That is consistent until something introduces a tenant, at
/// which point previously-written rows stop decrypting. Reported as info.
/// Field-name words that suggest a classification stronger than `none`.
/// Matching is done on lowercased words split from camelCase / snake_case,
/// including joined adjacent pairs (`first_name` -> `firstname`), to keep
/// false positives down.
const PII_WORDS: &[&str] = &[
    "email",
    "phone",
    "mobile",
    "address",
    "street",
    "zipcode",
    "postalcode",
    "firstname",
    "lastname",
    "fullname",
    "surname",
    "birth",
    "birthdate",
    "birthday",
    "dob",
    "gender",
    "nationality",
    "passport",
    "avatar",
    "photo",
    "latitude",
    "longitude",
    "geolocation",
    "ipaddress",
];

const PHI_WORDS: &[&str] = &[
    "ssn",
    "medical",
    "diagnosis",
    "prescription",
    "health",
    "bloodtype",
    "allergy",
    "allergies",
    "disability",
];

const PCI_WORDS: &[&str] = &[
    "cardnumber",
    "creditcard",
    "pan",
    "cvv",
    "cvc",
    "iban",
    "accountnumber",
    "routingnumber",
    "cardexpiry",
];

/// Split an identifier into lowercase words plus joined adjacent pairs.
fn identifier_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in name.chars() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                words.push(current.to_lowercase());
                current = String::new();
            }
        } else if c.is_uppercase() && !current.is_empty() {
            words.push(current.to_lowercase());
            current = c.to_string();
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }
    let mut all = words.clone();
    for pair in words.windows(2) {
        all.push(format!("{}{}", pair[0], pair[1]));
    }
    all
}

/// Suggest a classification for a field name, if any keyword matches.
pub(crate) fn suggest_classification(field_name: &str) -> Option<&'static str> {
    let words = identifier_words(field_name);
    // Strongest classification wins: PCI, then PHI, then PII.
    for word in &words {
        if PCI_WORDS.contains(&word.as_str()) {
            return Some("pci");
        }
    }
    for word in &words {
        if PHI_WORDS.contains(&word.as_str()) {
            return Some("phi");
        }
    }
    for word in &words {
        if PII_WORDS.contains(&word.as_str()) {
            return Some("pii");
        }
    }
    None
}

/// Which tenant gates an entrypoint source is missing.
pub(crate) fn missing_tenant_gates(source: &str) -> Vec<&'static (&'static str, &'static str)> {
    TENANT_GATES
        .iter()
        .filter(|(call, _)| !source.contains(call))
        .collect()
}

/// Run all local checks for every project under `modules_path`.
pub(crate) fn run_local_checks(modules_path: &Path) -> Result<Vec<LocalFinding>> {
    let mut findings: Vec<LocalFinding> = Vec::new();

    if !modules_path.exists() {
        return Ok(findings);
    }

    for entry in fs::read_dir(modules_path)? {
        let entry = entry?;
        let project_path = entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let project = entry.file_name().to_string_lossy().to_string();

        let owns_persistence = project_path.join("persistence").is_dir();

        // 1. Tenant-isolation wiring in every runtime entrypoint
        if owns_persistence {
            for entrypoint in ENTRYPOINTS {
                let entrypoint_path = project_path.join(entrypoint);
                if !entrypoint_path.exists() {
                    continue;
                }
                let source = fs::read_to_string(&entrypoint_path)?;
                for (_, consequence) in missing_tenant_gates(&source) {
                    findings.push(LocalFinding {
                        severity: Severity::Warning,
                        project: project.clone(),
                        check: "tenant-isolation-wiring".to_string(),
                        subject: (*entrypoint).to_string(),
                        message: format!("{} {}", entrypoint, consequence),
                    });
                }
            }
        }

        // 2. Retention / erasure service registration
        let entities = scan_entity_compliance(&project_path).unwrap_or_default();
        if !entities.is_empty() {
            let has_retention = entities.iter().any(|e| e.retention.is_some());
            let has_sensitive = entities.iter().any(|e| {
                e.field_classifications
                    .values()
                    .any(|classification| classification != "none")
            });
            let registrations =
                fs::read_to_string(project_path.join("registrations.ts")).unwrap_or_default();
            if has_retention && !registrations.contains("RetentionService") {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "retention-wiring".to_string(),
                    subject: "registrations.ts".to_string(),
                    message: "entities declare retention policies but RetentionService is not registered — nothing enforces them".to_string(),
                });
            }
            if has_sensitive && !registrations.contains("ComplianceDataService") {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "erasure-wiring".to_string(),
                    subject: "registrations.ts".to_string(),
                    message: "entities hold classified data but ComplianceDataService is not registered — erasure/export requests cannot be served".to_string(),
                });
            }

            // 3. Better Auth reading encrypted columns without a tenant context
            let better_auth_sources = BETTER_AUTH_WIRING_FILES
                .iter()
                .map(|file| fs::read_to_string(project_path.join(file)).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n");
            if has_sensitive && better_auth_orm_is_unwrapped(&better_auth_sources) {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "better-auth-encryption-context".to_string(),
                    subject: "registrations.ts".to_string(),
                    message: "entities hold classified data but Better Auth is wired to the raw ORM — its reads run with no tenant encryption context and will fail to decrypt once a tenant id is used".to_string(),
                });
            }

            // 4. Encryptor registration for classified columns.
            //
            // Gated on the project owning a MikroORM config, not merely having
            // a `persistence` directory. `defineComplianceEntity` is also used
            // to type queue payloads — forklaunch-platform's
            // deployment-agent-worker declares an event record with `pci`
            // fields, imports it with `import type`, and never persists it
            // through an ORM. Without this gate the check reports a plaintext
            // column on a table that does not exist.
            let owns_orm = project_path.join("mikro-orm.config.ts").exists();
            if has_sensitive && owns_orm {
                let sources = read_production_sources(&project_path);
                if !sources.contains("registerEncryptor(") {
                    findings.push(LocalFinding {
                        severity: Severity::Warning,
                        project: project.clone(),
                        check: "encryptor-registration".to_string(),
                        subject: "mikro-orm.config.ts".to_string(),
                        message: "entities declare classified fields but registerEncryptor() is never called — those columns are written as plaintext".to_string(),
                    });
                }

                let binds_tenant = TENANT_BINDING_MARKERS
                    .iter()
                    .any(|marker| sources.contains(marker));
                if !binds_tenant {
                    findings.push(LocalFinding {
                        severity: Severity::Info,
                        project: project.clone(),
                        check: "tenant-em-wiring".to_string(),
                        subject: "registrations.ts".to_string(),
                        message: "entities hold classified data but no encryption tenant is ever bound — encrypted columns can only use the no-tenant key, and will stop decrypting if a tenant is introduced later. Note this is about the ENCRYPTION context specifically: setFilterParams('tenant', ..) scopes queries but does not set it".to_string(),
                    });
                }

                // The factory binds the tenant FILTER but not the tenant
                // ENCRYPTION context — the divergence that makes every later
                // read in the service fragile.
                if sources.contains(TENANT_FILTER_ONLY_MARKER)
                    && !sources.contains(TENANT_CONTEXT_HELPER)
                {
                    findings.push(LocalFinding {
                        severity: Severity::Warning,
                        project: project.clone(),
                        check: "tenant-context-half-wired".to_string(),
                        subject: "registrations.ts".to_string(),
                        message: "the EntityManager factory calls setFilterParams('tenant', ..) but never wrapEmWithTenantContext — callers get row filtering and believe they are tenant-scoped, while encrypted columns still decrypt with the no-tenant key".to_string(),
                    });
                }
            }

            // 5. Sensitive-field heuristics
            for entity in &entities {
                for (field, classification) in &entity.field_classifications {
                    if classification != "none" {
                        continue;
                    }
                    if let Some(suggested) = suggest_classification(field) {
                        findings.push(LocalFinding {
                            severity: Severity::Info,
                            project: project.clone(),
                            check: "possible-misclassification".to_string(),
                            subject: format!("{}.{}", entity.entity_name, field),
                            message: format!(
                                "field name suggests '{}' data but it is classified 'none' — review the classification",
                                suggested
                            ),
                        });
                    }
                }
            }
        }
    }

    // Deterministic output for --json / snapshots
    findings.sort_by(|a, b| {
        (&a.project, &a.check, &a.subject, &a.message)
            .cmp(&(&b.project, &b.check, &b.subject, &b.message))
    });
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_tenant_gates_flags_absent_calls() {
        let source = "const orm = ci.resolve(tokens.Orm);";
        let missing = missing_tenant_gates(source);
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn test_missing_tenant_gates_passes_wired_entrypoint() {
        let source = r#"
            setupTenantFilter(orm, { logger });
            setupRls(orm, { logger });
        "#;
        assert!(missing_tenant_gates(source).is_empty());
    }

    #[test]
    fn test_better_auth_orm_unwrapped_is_flagged() {
        // The shape that took production down: Better Auth handed `Orm`
        // directly while the service's own EntityManager is tenant-wrapped.
        let registrations = r#"
            EntityManager: { factory: ({ Orm }, context) =>
                wrapEmWithTenantContext(Orm.em.fork(), context?.tenantId) },
            BetterAuth: { factory: ({ Orm }) =>
                betterAuth(betterAuthConfig({ orm: Orm })) }
        "#;
        assert!(better_auth_orm_is_unwrapped(registrations));
    }

    #[test]
    fn test_better_auth_orm_wrapped_passes() {
        let registrations = r#"
            BetterAuth: { factory: ({ Orm }) =>
                betterAuth(betterAuthConfig({ orm: createEncryptionAwareOrm(Orm) })) }
        "#;
        assert!(!better_auth_orm_is_unwrapped(registrations));
    }

    #[test]
    fn test_better_auth_platform_proxy_also_counts_as_wrapped() {
        // forklaunch-platform wraps via its own proxy rather than the
        // blueprint helper; both make the reads encryption-aware.
        let registrations = r#"
            orm: createTenantAwareBetterAuthOrmProxy(Orm)
        "#;
        assert!(!better_auth_orm_is_unwrapped(registrations));
    }

    #[test]
    fn test_wrapper_in_auth_ts_counts_even_when_registrations_looks_raw() {
        // The exact false positive dogfooding caught: forklaunch-platform
        // passes `orm: Orm` at the registration site and applies the proxy
        // inside auth.ts. Searching registrations.ts alone flags a service
        // that is correctly wired.
        let registrations = "betterAuth(betterAuthConfig({ orm: Orm }))";
        let auth = "const betterAuthOrm = createTenantAwareBetterAuthOrmProxy(orm);";
        let combined = format!("{}\n{}", registrations, auth);

        assert!(better_auth_orm_is_unwrapped(registrations));
        assert!(!better_auth_orm_is_unwrapped(&combined));
    }

    #[test]
    fn test_project_without_better_auth_is_not_flagged() {
        // A service that never wires Better Auth has nothing to answer for —
        // the check must not fire on every project that owns entities.
        let registrations = r#"
            EntityManager: { factory: ({ Orm }, context) =>
                wrapEmWithTenantContext(Orm.em.fork(), context?.tenantId) }
        "#;
        assert!(!better_auth_orm_is_unwrapped(registrations));
    }

    #[test]
    fn test_suggest_classification_pii_camel_and_snake() {
        assert_eq!(suggest_classification("email"), Some("pii"));
        assert_eq!(suggest_classification("userEmail"), Some("pii"));
        assert_eq!(suggest_classification("first_name"), Some("pii"));
        assert_eq!(suggest_classification("billingAddress"), Some("pii"));
        assert_eq!(suggest_classification("dateOfBirth"), Some("pii"));
        assert_eq!(suggest_classification("birthDate"), Some("pii"));
    }

    #[test]
    fn test_suggest_classification_strongest_wins() {
        assert_eq!(suggest_classification("cardNumber"), Some("pci"));
        assert_eq!(suggest_classification("ssn"), Some("phi"));
        assert_eq!(suggest_classification("healthInsuranceNumber"), Some("phi"));
    }

    #[test]
    fn test_suggest_classification_avoids_generic_names() {
        assert_eq!(suggest_classification("name"), None);
        assert_eq!(suggest_classification("description"), None);
        assert_eq!(suggest_classification("externalId"), None);
        assert_eq!(suggest_classification("billingProvider"), None);
        assert_eq!(suggest_classification("cadence"), None);
    }
}

#[cfg(test)]
mod fs_tests {
    use super::*;

    #[test]
    fn test_run_local_checks_flags_unwired_entrypoint() {
        let dir = std::env::temp_dir().join("fl-checks-test");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(proj.join("persistence")).unwrap();
        std::fs::write(proj.join("server.ts"), "const app = express();").unwrap();
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            findings.len(),
            2,
            "expected 2 wiring findings, got: {:?}",
            findings
        );
        assert!(
            findings
                .iter()
                .all(|f| f.check == "tenant-isolation-wiring")
        );
    }

    /// Builds a project whose entity holds classified data, so the
    /// encryption-context check has something to fire on.
    fn write_project_with_classified_entity(proj: &std::path::Path, registrations: &str) {
        std::fs::create_dir_all(proj.join("persistence/entities")).unwrap();
        std::fs::write(
            proj.join("persistence/entities/account.entity.ts"),
            r#"
            export const AccountEntity = defineComplianceEntity({
              name: 'Account',
              properties: {
                accountId: fp.string().compliance('none'),
                password: fp.string().nullable().compliance('pii')
              }
            });
            "#,
        )
        .unwrap();
        std::fs::write(proj.join("registrations.ts"), registrations).unwrap();
        // Owning an ORM is what makes the encryption checks applicable.
        std::fs::write(proj.join("mikro-orm.config.ts"), "export default {};").unwrap();
    }

    #[test]
    fn test_run_local_checks_flags_missing_encryptor() {
        let dir = std::env::temp_dir().join("fl-checks-no-encryptor");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(&proj, "export const x = 1;");

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let f = findings
            .iter()
            .find(|f| f.check == "encryptor-registration")
            .unwrap_or_else(|| panic!("expected the finding, got: {:?}", findings));
        // Plaintext at rest in a column declared pii is a warning, not info.
        assert!(matches!(f.severity, Severity::Warning));
    }

    #[test]
    fn test_run_local_checks_accepts_encryptor_in_mikro_orm_config() {
        let dir = std::env::temp_dir().join("fl-checks-encryptor-ok");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(&proj, "export const x = 1;");
        // Every blueprint registers it here, not in registrations.ts.
        std::fs::write(
            proj.join("mikro-orm.config.ts"),
            "registerEncryptor(new FieldEncryptor(key));",
        )
        .unwrap();

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !findings.iter().any(|f| f.check == "encryptor-registration"),
            "registering in mikro-orm.config.ts should satisfy the check, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_run_local_checks_reports_unbound_tenant_em_as_info() {
        let dir = std::env::temp_dir().join("fl-checks-no-tenant-em");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(&proj, "export const x = 1;");

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let f = findings
            .iter()
            .find(|f| f.check == "tenant-em-wiring")
            .unwrap_or_else(|| panic!("expected the finding, got: {:?}", findings));
        // A deliberately single-tenant service is not broken, so this must not
        // be a warning — the helper no-ops when the tenant id is undefined.
        assert!(matches!(f.severity, Severity::Info));
    }

    #[test]
    fn test_flags_factory_that_filters_without_binding_encryption() {
        // The exact drift that took down sign-up, /me and an invitation lookup
        // in one week: filter params set, encryption tenant never bound.
        let dir = std::env::temp_dir().join("fl-checks-half-wired");
        let proj = dir.join("iam");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(
            &proj,
            "registerEncryptor(enc); em.setFilterParams('tenant', { tenantId });",
        );

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            findings
                .iter()
                .any(|f| f.check == "tenant-context-half-wired"),
            "expected the half-wired finding, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_factory_using_the_helper_is_not_flagged() {
        // What every generated blueprint does, and what the drifted services
        // should return to. Both markers present must NOT flag.
        let dir = std::env::temp_dir().join("fl-checks-fully-wired");
        let proj = dir.join("iam");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(
            &proj,
            "registerEncryptor(enc); wrapEmWithTenantContext(Orm.em.fork(), context?.tenantId);",
        );

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !findings
                .iter()
                .any(|f| f.check == "tenant-context-half-wired"),
            "a factory using the helper must not be flagged, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_entities_without_an_orm_are_not_flagged() {
        // The false positive found by running this against forklaunch-platform:
        // deployment-agent-worker types its BullMQ payload with
        // defineComplianceEntity, marks fields `pci`, imports it with
        // `import type`, and owns no mikro-orm.config.ts. Nothing is persisted,
        // so there is no plaintext column to report.
        let dir = std::env::temp_dir().join("fl-checks-queue-payload");
        let proj = dir.join("worker");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(&proj, "export const x = 1;");
        // The helper writes one; a queue-payload module has none.
        std::fs::remove_file(proj.join("mikro-orm.config.ts")).unwrap();

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !findings
                .iter()
                .any(|f| f.check == "encryptor-registration" || f.check == "tenant-em-wiring"),
            "a project with no ORM should not be flagged, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_tenant_binding_accepts_with_encryption_context() {
        // The false positive dogfooding caught: forklaunch-platform's IAM never
        // calls wrapEmWithTenantContext — it enters the context directly around
        // its reads. Insisting on one helper flags a service that does bind.
        let dir = std::env::temp_dir().join("fl-checks-alt-binding");
        let proj = dir.join("iam");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(&proj, "registerEncryptor(enc);");
        std::fs::write(
            proj.join("auth.ts"),
            "await withEncryptionContext(orgId, () => em.findOne(Account, where));",
        )
        .unwrap();

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !findings.iter().any(|f| f.check == "tenant-em-wiring"),
            "withEncryptionContext should count as binding, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_tenant_binding_in_a_test_file_does_not_count() {
        // A project whose only binding call lives in a test is not wired. This
        // is how the platform's IAM first looked wired under a naive grep.
        let dir = std::env::temp_dir().join("fl-checks-test-only-binding");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(&proj, "registerEncryptor(enc);");
        std::fs::create_dir_all(proj.join("__test__")).unwrap();
        std::fs::write(
            proj.join("__test__/thing.test.ts"),
            "withEncryptionContext('org', () => {});",
        )
        .unwrap();

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            findings.iter().any(|f| f.check == "tenant-em-wiring"),
            "a binding call only in tests must still flag, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_run_local_checks_ignores_projects_without_classified_data() {
        let dir = std::env::temp_dir().join("fl-checks-unclassified");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(proj.join("persistence/entities")).unwrap();
        std::fs::write(
            proj.join("persistence/entities/thing.entity.ts"),
            "export const ThingEntity = defineComplianceEntity({ name: 'Thing', properties: { label: fp.string().compliance('none') } });",
        )
        .unwrap();
        std::fs::write(proj.join("registrations.ts"), "export const x = 1;").unwrap();

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        // Nothing classified means nothing to encrypt or bind — neither check
        // should fire, or every plain CRUD service gets noise.
        assert!(
            !findings
                .iter()
                .any(|f| f.check == "encryptor-registration" || f.check == "tenant-em-wiring"),
            "unclassified project should be quiet, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_run_local_checks_flags_better_auth_raw_orm() {
        let dir = std::env::temp_dir().join("fl-checks-ba-raw");
        let proj = dir.join("iam");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(
            &proj,
            "BetterAuth: betterAuth(betterAuthConfig({ orm: Orm }))",
        );

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            findings
                .iter()
                .any(|f| f.check == "better-auth-encryption-context"),
            "expected the encryption-context finding, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_run_local_checks_passes_encryption_aware_better_auth() {
        let dir = std::env::temp_dir().join("fl-checks-ba-wrapped");
        let proj = dir.join("iam");
        let _ = std::fs::remove_dir_all(&dir);
        write_project_with_classified_entity(
            &proj,
            "BetterAuth: betterAuth(betterAuthConfig({ orm: createEncryptionAwareOrm(Orm) }))",
        );

        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !findings
                .iter()
                .any(|f| f.check == "better-auth-encryption-context"),
            "wrapped ORM should not be flagged, got: {:?}",
            findings
        );
    }
}
