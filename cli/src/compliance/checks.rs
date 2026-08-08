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
            let registrations = fs::read_to_string(project_path.join("registrations.ts"))
                .unwrap_or_default();
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

            // 3. Sensitive-field heuristics
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
        (&a.project, &a.check, &a.subject, &a.message).cmp(&(
            &b.project,
            &b.check,
            &b.subject,
            &b.message,
        ))
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
        assert_eq!(findings.len(), 2, "expected 2 wiring findings, got: {:?}", findings);
        assert!(findings.iter().all(|f| f.check == "tenant-isolation-wiring"));
    }
}
