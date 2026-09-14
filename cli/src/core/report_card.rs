//! Build an Enterprise-Readiness Report Card from deterministic checks alone.
//!
//! The card contract (`REPORT_CARD_SCHEMA_VERSION` 2, five weighted rails) is
//! defined once in TypeScript and shared by every surface that produces one.
//! That file names this producer explicitly: "`forklaunch audit` repo analysis
//! (deterministic compliance findings → card)", reserves `phase: 'audit'` for
//! it, and tags findings `source: 'cli'` to separate them from AI assessment.
//! This module is the Rust side of that contract.
//!
//! # What this can and cannot score
//!
//! Two rails are decidable from static analysis, and three are not:
//!
//! | rail | scored here | why |
//! |---|---|---|
//! | compliance | yes | encryptor registration, retention + erasure wiring, field classification |
//! | security | yes | tenant isolation and encryption-context wiring |
//! | governance | no | needs judgement about process and ownership |
//! | scalability | no | needs load characteristics no source read reveals |
//! | observability | no | wiring is visible, but whether it covers what matters is not |
//!
//! Emitting 0 for the three would read as failure rather than absence, so they
//! are marked `pending` and excluded from the weighted average, and the card
//! carries a `caveat` saying so. `--upload` replaces this with the platform's
//! full agent-scored card.

use serde::Serialize;


use crate::compliance::checks::{LocalFinding, Severity};

/// Bump only alongside the TypeScript `REPORT_CARD_SCHEMA_VERSION`.
const SCHEMA_VERSION: u32 = 2;

/// Rails this command can decide from source alone, with their contract weights.
const SCORED_RAILS: &[(&str, f64)] = &[("compliance", 0.25), ("security", 0.25)];

/// Rails that need an agent. Weights are carried for documentation only; they
/// are excluded from the average rather than counted as zero.
const PENDING_RAILS: &[(&str, f64, &str)] = &[
    (
        "governance",
        0.15,
        "Ownership, review and change-control practice are not visible in source.",
    ),
    (
        "scalability",
        0.15,
        "Load characteristics and data growth are not visible in source.",
    ),
    (
        "observability",
        0.2,
        "Instrumentation wiring is visible, but whether it covers what matters is a judgement.",
    ),
];

/// Which rail each deterministic check belongs to.
fn rail_for_check(check: &str) -> &'static str {
    match check {
        "tenant-isolation-wiring" | "tenant-em-wiring" | "tenant-context-half-wired" => "security",
        _ => "compliance",
    }
}

/// Contract severity for a local finding.
///
/// `tenant-context-half-wired` is promoted above the other warnings on
/// evidence: it is the one check whose failure mode is silent — rows filter
/// correctly, tests pass, and encrypted columns quietly use the wrong key until
/// a real tenant exists in production.
fn severity_for(finding: &LocalFinding) -> &'static str {
    match (&finding.severity, finding.check.as_str()) {
        (Severity::Warning, "tenant-context-half-wired") => "critical",
        (Severity::Warning, _) => "high",
        (Severity::Info, _) => "info",
    }
}

/// Points deducted per finding. Deductions accumulate but never drive a rail
/// below zero.
fn penalty_for(severity: &str) -> f64 {
    match severity {
        "critical" => 30.0,
        "high" => 15.0,
        "medium" => 8.0,
        "low" => 4.0,
        _ => 0.0,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardFinding {
    pub(crate) severity: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fix: Option<String>,
    /// Always `cli` here: these are deterministic, not model output.
    pub(crate) source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardItem {
    pub(crate) label: String,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardDimension {
    pub(crate) score: u32,
    pub(crate) summary: String,
    pub(crate) items: Vec<CardItem>,
    /// The CLI asks nothing; the field is required by the contract.
    pub(crate) questions: Vec<serde_json::Value>,
    pub(crate) findings: Vec<CardFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pending: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportCard {
    pub(crate) schema_version: u32,
    pub(crate) overall: u32,
    pub(crate) headline: String,
    pub(crate) caveat: String,
    pub(crate) phase: String,
    pub(crate) step: String,
    /// Empty: deciding which frameworks apply to a domain is an agent's job,
    /// and guessing would be worse than saying nothing.
    pub(crate) frameworks: Vec<String>,
    pub(crate) dimensions: std::collections::BTreeMap<String, CardDimension>,
    pub(crate) generated_at: String,
}

/// One human-readable line per check, used for the rail checklist.
fn item_label(check: &str) -> &'static str {
    match check {
        "encryptor-registration" => "Field encryptor is registered",
        "retention-wiring" => "Retention policies are wired",
        "erasure-wiring" => "GDPR erasure is wired",
        "possible-misclassification" => "Sensitive fields are classified",
        "better-auth-encryption-context" => "Better Auth reads bind an encryption context",
        "tenant-isolation-wiring" => "Tenant isolation filter is installed",
        "tenant-em-wiring" => "An encryption tenant is bound",
        "tenant-context-half-wired" => "Tenant filter and encryption context agree",
        _ => "Deterministic check",
    }
}

fn remedy(check: &str) -> Option<String> {
    let text = match check {
        "tenant-context-half-wired" => {
            "Replace the hand-rolled fork with \
             wrapEmWithTenantContext(Orm.em.fork(opts), context?.tenantId). setFilterParams \
             scopes rows only; it does not bind the encryption key."
        }
        "tenant-em-wiring" | "tenant-isolation-wiring" => {
            "Bind the tenant on the EntityManager before reading encrypted columns."
        }
        "encryptor-registration" => "Register a FieldEncryptor during bootstrap.",
        "retention-wiring" => "Declare retention on classified entities.",
        "erasure-wiring" => "Wire the erasure handler so subject-deletion requests are honoured.",
        "possible-misclassification" => {
            "Classify the field with .compliance('pii'|'phi'|'pci') or confirm 'none' is correct."
        }
        "better-auth-encryption-context" => {
            "Wrap Better Auth's EntityManager so its reads carry the tenant."
        }
        _ => return None,
    };
    Some(text.to_string())
}

/// Build a card from deterministic findings.
///
/// `generated_at` is passed in rather than read from the clock so the caller
/// owns time and this stays a pure function.
pub(crate) fn build_local_report_card(
    app_name: &str,
    module_count: usize,
    findings: &[LocalFinding],
    generated_at: String,
) -> ReportCard {
    let mut dimensions = std::collections::BTreeMap::new();
    let mut weighted_total = 0.0;
    let mut weight_used = 0.0;

    for (rail, weight) in SCORED_RAILS {
        let rail_findings: Vec<&LocalFinding> = findings
            .iter()
            .filter(|f| rail_for_check(&f.check) == *rail)
            .collect();

        let mut score = 100.0;
        let mut card_findings = Vec::new();
        for finding in &rail_findings {
            let severity = severity_for(finding);
            score -= penalty_for(severity);
            card_findings.push(CardFinding {
                severity: severity.to_string(),
                title: format!("{} ({})", item_label(&finding.check), finding.project),
                detail: finding.message.clone(),
                fix: remedy(&finding.check),
                source: "cli".to_string(),
            });
        }
        let score = score.max(0.0);

        // A check that fired is unmet; one that never fired passed everywhere
        // it was evaluated. Only checks belonging to this rail are listed.
        let mut items: Vec<CardItem> = Vec::new();
        for check in all_checks_for_rail(rail) {
            let failing: Vec<&&LocalFinding> =
                rail_findings.iter().filter(|f| f.check == *check).collect();
            items.push(CardItem {
                label: item_label(check).to_string(),
                status: if failing.is_empty() { "met" } else { "unmet" }.to_string(),
                detail: failing.first().map(|f| f.message.clone()),
            });
        }

        let summary = if card_findings.is_empty() {
            format!("No deterministic {rail} findings across {module_count} module(s).")
        } else {
            format!(
                "{} deterministic finding(s) across {} module(s).",
                card_findings.len(),
                module_count
            )
        };

        dimensions.insert(
            (*rail).to_string(),
            CardDimension {
                score: score.round() as u32,
                summary,
                items,
                questions: Vec::new(),
                findings: card_findings,
                pending: None,
            },
        );

        weighted_total += score * weight;
        weight_used += weight;
    }

    for (rail, _weight, why) in PENDING_RAILS {
        dimensions.insert(
            (*rail).to_string(),
            CardDimension {
                score: 0,
                summary: format!("Not assessed. {why}"),
                items: Vec::new(),
                questions: Vec::new(),
                findings: Vec::new(),
                pending: Some(true),
            },
        );
    }

    // Renormalise over the rails actually scored. Counting the unassessed
    // three as zero would cap this card at 50 and read as failure rather than
    // absence.
    let overall = if weight_used > 0.0 {
        (weighted_total / weight_used).round() as u32
    } else {
        0
    };

    let total = findings.len();
    let headline = if total == 0 {
        format!("{app_name}: no deterministic compliance or security findings across {module_count} module(s).")
    } else {
        format!(
            "{app_name}: {total} deterministic finding(s) across {module_count} module(s), scored on compliance and security."
        )
    };

    ReportCard {
        schema_version: SCHEMA_VERSION,
        overall,
        headline,
        caveat: "Deterministic checks only. Compliance and security are scored from static \
                 analysis; governance, scalability and observability need judgement a source \
                 read cannot supply and are left unassessed rather than scored zero. The \
                 overall score is weighted across the scored rails only. All five rails need \
                 an agent's judgement, which the studio surface provides."
            .to_string(),
        phase: "audit".to_string(),
        step: "audit".to_string(),
        frameworks: Vec::new(),
        dimensions,
        generated_at,
    }
}

/// Every check that can contribute to a rail, so the checklist shows passes as
/// well as failures. A rail whose checks all pass should say so explicitly.
fn all_checks_for_rail(rail: &str) -> &'static [&'static str] {
    match rail {
        "security" => &[
            "tenant-isolation-wiring",
            "tenant-em-wiring",
            "tenant-context-half-wired",
        ],
        _ => &[
            "encryptor-registration",
            "retention-wiring",
            "erasure-wiring",
            "possible-misclassification",
            "better-auth-encryption-context",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(check: &str, severity: Severity) -> LocalFinding {
        LocalFinding {
            severity,
            project: "iam".to_string(),
            check: check.to_string(),
            subject: "registrations.ts".to_string(),
            message: "example".to_string(),
        }
    }

    fn at() -> String {
        "2026-01-01T00:00:00Z".to_string()
    }

    #[test]
    fn a_clean_workspace_scores_full_marks_on_the_rails_it_can_judge() {
        let card = build_local_report_card("demo", 3, &[], at());
        assert_eq!(card.overall, 100);
        assert_eq!(card.dimensions["compliance"].score, 100);
        assert_eq!(card.dimensions["security"].score, 100);
    }

    #[test]
    fn unassessed_rails_are_pending_not_zero_scores() {
        // Scoring them 0 would cap every card at 50 and read as failure rather
        // than absence -- the distinction the `pending` flag exists to make.
        let card = build_local_report_card("demo", 1, &[], at());
        for rail in ["governance", "scalability", "observability"] {
            assert_eq!(card.dimensions[rail].pending, Some(true), "{rail}");
        }
        assert_eq!(
            card.overall, 100,
            "unassessed rails must not drag the average down"
        );
    }

    #[test]
    fn the_silent_failure_check_outweighs_the_others() {
        let half_wired = build_local_report_card(
            "demo",
            1,
            &[finding("tenant-context-half-wired", Severity::Warning)],
            at(),
        );
        let ordinary =
            build_local_report_card("demo", 1, &[finding("tenant-em-wiring", Severity::Warning)], at());

        assert!(
            half_wired.dimensions["security"].score < ordinary.dimensions["security"].score,
            "the check whose failure mode is silent must cost more"
        );
        assert_eq!(
            half_wired.dimensions["security"].findings[0].severity,
            "critical"
        );
    }

    #[test]
    fn findings_route_to_the_right_rail() {
        let card = build_local_report_card(
            "demo",
            1,
            &[
                finding("tenant-em-wiring", Severity::Warning),
                finding("retention-wiring", Severity::Warning),
            ],
            at(),
        );
        assert_eq!(card.dimensions["security"].findings.len(), 1);
        assert_eq!(card.dimensions["compliance"].findings.len(), 1);
    }

    #[test]
    fn a_rail_never_scores_below_zero() {
        let many: Vec<LocalFinding> = (0..20)
            .map(|_| finding("tenant-context-half-wired", Severity::Warning))
            .collect();
        let card = build_local_report_card("demo", 1, &many, at());
        assert_eq!(card.dimensions["security"].score, 0);
    }

    #[test]
    fn info_findings_are_reported_without_costing_points() {
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("possible-misclassification", Severity::Info)],
            at(),
        );
        assert_eq!(card.dimensions["compliance"].score, 100);
        assert_eq!(card.dimensions["compliance"].findings.len(), 1);
        assert_eq!(card.dimensions["compliance"].findings[0].severity, "info");
    }

    #[test]
    fn passing_checks_are_listed_as_met_so_the_checklist_shows_work_done() {
        let card = build_local_report_card("demo", 1, &[], at());
        let security = &card.dimensions["security"];
        assert!(!security.items.is_empty());
        assert!(security.items.iter().all(|i| i.status == "met"));
    }

    #[test]
    fn every_finding_carries_a_remedy_and_is_marked_deterministic() {
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("tenant-context-half-wired", Severity::Warning)],
            at(),
        );
        let f = &card.dimensions["security"].findings[0];
        assert_eq!(f.source, "cli");
        assert!(f.fix.as_ref().is_some_and(|s| s.contains("wrapEmWithTenantContext")));
    }

    #[test]
    fn the_card_declares_the_contract_version_and_audit_phase() {
        let card = build_local_report_card("demo", 1, &[], at());
        assert_eq!(card.schema_version, SCHEMA_VERSION);
        assert_eq!(card.phase, "audit");
        // The caveat is the only thing standing between a partial score and
        // someone quoting it as a full readiness number, so it must say both
        // that rails were skipped and where the rest come from -- without
        // naming a flag this command does not have.
        assert!(
            card.caveat.contains("unassessed"),
            "the caveat must say some rails were not scored: {}",
            card.caveat
        );
        assert!(
            card.caveat.contains("studio"),
            "the caveat must say where the remaining rails come from: {}",
            card.caveat
        );
        assert!(
            !card.caveat.contains("--upload"),
            "the caveat must not name a flag `score` does not accept"
        );
    }
}

/// UTC timestamp for the card's `generatedAt`, without pulling in a date crate.
pub(crate) fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Days since epoch -> civil date (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

