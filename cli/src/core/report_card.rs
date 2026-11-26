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
//! Every deterministic check bears on one platform checklist criterion (the
//! `LOCAL_CHECKS` table in the platform's `run-checks.ts`), and each item on
//! this card carries that criterion's id and its exact label, so the offline
//! card lists the same lines as the website's "Checked" half. A criterion's
//! rail decides where its item lands:
//!
//! | rail | scored here | criteria |
//! |---|---|---|
//! | compliance | yes | `cmp-encryption-at-rest`, `cmp-data-classification` |
//! | security | yes | `sec-tenant-isolation` |
//! | governance | yes | `gov-data-retention`, `gdpr-erasure` |
//! | scalability | no | needs load characteristics no source read reveals |
//! | observability | no | wiring is visible, but whether it covers what matters is not |
//!
//! Emitting 0 for the last two would read as failure rather than absence, so
//! they are marked `pending` and excluded from the weighted average, and the
//! card carries a `caveat` saying so. The online `score` replaces this with the
//! platform's full agent-scored card.
//!
//! # Nothing to read is not a pass
//!
//! A check that fires nothing has passed only where it had something to look
//! at. When the workspace holds none of what a criterion's checks read (no
//! routes, no classified entities, no ForkLaunch-wired project), the item is
//! `unassessed` and scores zero: an app that never adopted the framework has
//! not shown that every endpoint requires sign-in.
//!
//! The same holds for a Rust service whose evidence a check does not read.
//! Routes and data classifications are read from Rust source, but most of the
//! entity-driven checks look only for TypeScript wiring. Where no check on an
//! item reads Rust, the item is `unassessed` with a reason that says so, and a
//! TypeScript-only check on an otherwise decided item is `skipped`, not `pass`.

use serde::Serialize;

use crate::compliance::checks::{LocalEvidence, LocalFinding, Severity};

/// Bump only alongside the TypeScript `REPORT_CARD_SCHEMA_VERSION`.
const SCHEMA_VERSION: u32 = 2;

/// Rails this command can decide from source alone, with their contract weights.
const SCORED_RAILS: &[(&str, f64)] = &[
    ("compliance", 0.25),
    ("security", 0.25),
    ("governance", 0.15),
];

/// Rails that need an agent. Weights are carried for documentation only; they
/// are excluded from the average rather than counted as zero.
const PENDING_RAILS: &[(&str, f64, &str)] = &[
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

/// A platform checklist criterion (`BASELINE_CRITERIA` / `FRAMEWORK_CRITERIA`
/// in the platform's `report-card.ts`). Labels are the platform's exact
/// strings: the website matches checks to items by label, so a paraphrase here
/// would put the offline card and the website on different lines.
struct Criterion {
    id: &'static str,
    rail: &'static str,
    label: &'static str,
}

/// The criteria the offline checks bear on, in the order the card lists them.
const CRITERIA: &[Criterion] = &[
    Criterion {
        id: "cmp-encryption-at-rest",
        rail: "compliance",
        label: "Sensitive fields are encrypted at rest",
    },
    Criterion {
        id: "cmp-data-classification",
        rail: "compliance",
        label: "Sensitive data is identified and handled according to its sensitivity",
    },
    Criterion {
        id: "sec-tenant-isolation",
        rail: "security",
        label: "One customer cannot see or change another customer\u{2019}s data",
    },
    Criterion {
        id: "sec-authn",
        rail: "security",
        label: "Every endpoint requires sign-in unless it is explicitly public",
    },
    Criterion {
        id: "sec-dependencies",
        rail: "security",
        label: "No installed dependency has a known vulnerability or is malicious",
    },
    Criterion {
        id: "sec-input-validation",
        rail: "security",
        label: "Incoming data is validated before it is trusted",
    },
    Criterion {
        id: "gov-data-retention",
        rail: "governance",
        label: "How long data is kept, and how it is deleted, is defined",
    },
    Criterion {
        id: "gdpr-erasure",
        rail: "governance",
        label: "Users can request data export and erasure",
    },
    Criterion {
        id: "gov-construction",
        rail: "governance",
        label: "The app builds cleanly to its declared parameters (construction check passes)",
    },
];

/// Why the offline checks could not decide a criterion.
struct Unassessed {
    /// Shown on the checklist item.
    why: String,
    /// Shown on each of the item's checks.
    check: String,
    /// The reason is about Rust services (a check that does not read them yet,
    /// or a declaration source alone cannot decide), not an empty workspace.
    rust_unread: bool,
}

/// How a `route-scan-incomplete` finding starts when the file was read but a
/// route declared in it could not be decided.
const UNDECIDED_ROUTES: &str = crate::compliance::checks::UNDECIDED_ROUTES;
const NOTHING_TO_READ: &str = "Nothing for this check to read.";
const RUST_UNREAD: &str = "This check does not read Rust services yet.";
const UNBOUND: &str = "Not decided: a Rust type classifies members without a protected-model \
                       binding, and source alone does not show how they are stored.";

/// The criteria decided from classified entities.
const ENTITY_CRITERIA: &[&str] = &[
    "cmp-encryption-at-rest",
    "cmp-data-classification",
    "sec-tenant-isolation",
    "gov-data-retention",
    "gdpr-erasure",
];

/// Whether any check on `criterion` reads Rust entities.
fn reads_rust_entities(criterion: &str) -> bool {
    checks_for_criterion(criterion)
        .iter()
        .any(|check| crate::compliance::checks::RUST_ENTITY_CHECKS.contains(check))
}

/// Why the offline checks could not decide a criterion: the workspace holds
/// nothing they read, or holds it only in a form they do not read yet. `None`
/// when there was something to look at.
///
/// # Rust services
///
/// A check that only looks for TypeScript wiring has decided nothing about a
/// Rust entity, so finding nothing there is not a pass. A criterion none of
/// whose checks read Rust is unassessed as soon as any classified entity is
/// declared in Rust, even when TypeScript entities in the same app passed: the
/// item covers the whole app, and part of it was not read.
fn missing_evidence(criterion: &str, evidence: &LocalEvidence) -> Option<Unassessed> {
    let no_routes = "No routes were found for the offline checks to read, so nothing shows \
                     which endpoints require sign-in or validate their input. Declare routes \
                     with ForkLaunch contracts, or run `forklaunch score` online for a full \
                     source review.";
    let no_entities = "No entities with data classifications were found, so nothing shows how \
                       sensitive data is stored, isolated, kept or erased. Declare entities with \
                       ForkLaunch compliance fields, or run `forklaunch score` online for a full \
                       source review.";
    let no_projects = "No ForkLaunch-wired service or worker was found, so there is no declared \
                       construction to check. Run `forklaunch score` online for a full source \
                       review.";
    let nothing = |why: &str| {
        Some(Unassessed {
            why: why.to_string(),
            check: NOTHING_TO_READ.to_string(),
            rust_unread: false,
        })
    };
    let rust_unread = |why: &str| {
        Some(Unassessed {
            why: why.to_string(),
            check: RUST_UNREAD.to_string(),
            rust_unread: true,
        })
    };
    match criterion {
        "sec-authn" | "sec-input-validation" if evidence.routes == 0 => nothing(no_routes),
        // Routes were read, but every one is a raw Rust route: the framework
        // validates what a contract declares, and none of these has one.
        "sec-input-validation"
            if evidence.routes == evidence.rust_routes && evidence.rust_contract_routes == 0 =>
        {
            Some(Unassessed {
                why: "Every route found is registered directly on a Rust router, outside \
                      ForkLaunch contracts, so nothing the offline checks read shows that \
                      incoming data is validated. Declare the routes as contract endpoints, or \
                      run `forklaunch score` online for a full source review."
                    .to_string(),
                check: "No route is declared through a ForkLaunch contract, so this check \
                        passing would not show that input is validated."
                    .to_string(),
                rust_unread: false,
            })
        }
        id if ENTITY_CRITERIA.contains(&id) && evidence.entities == 0 => nothing(no_entities),
        // The Rust encryption check confirms a protected-model binding. A
        // classified type without one is neither confirmed nor refuted.
        "cmp-encryption-at-rest" if !evidence.rust_unbound.is_empty() => Some(Unassessed {
            why: format!(
                "{} Rust type(s) classify members without binding them to protected storage \
                 (no #[compliance(persisted)] model with ProtectedField members): {}. The \
                 source does not show whether those members are stored as ciphertext, so the \
                 offline check cannot decide this item. Bind them to a protected model, or run \
                 `forklaunch score` online for a full source review.",
                evidence.rust_unbound.len(),
                evidence.rust_unbound.join(", ")
            ),
            check: UNBOUND.to_string(),
            rust_unread: true,
        }),
        "sec-tenant-isolation" if evidence.rust_entities > 0 => rust_unread(
            "Classified entities are declared in Rust services, and the offline \
             tenant-isolation checks only read TypeScript wiring (setupTenantFilter, setupRls, \
             wrapEmWithTenantContext). The Rust check does not exist yet, so nothing read here \
             shows that the Rust services keep one customer's rows from another. Run \
             `forklaunch score` online for a full source review.",
        ),
        "gov-data-retention" if evidence.rust_entities > 0 => rust_unread(
            "Classified entities are declared in Rust services. Rust entities declare no \
             retention policy the offline checks can read (the framework's own compliance \
             artifact reports lifecycle coverage as unknown), and the offline retention check \
             only looks for the TypeScript RetentionService. The Rust check does not exist \
             yet. Run `forklaunch score` online for a full source review.",
        ),
        "gdpr-erasure" if evidence.rust_entities > 0 => rust_unread(
            "Classified entities are declared in Rust services, and the offline erasure check \
             only looks for the TypeScript ComplianceDataService. The Rust check does not \
             exist yet, so nothing read here shows that export and erasure requests reach the \
             data the Rust services hold. Run `forklaunch score` online for a full source \
             review.",
        ),
        id if ENTITY_CRITERIA.contains(&id)
            && evidence.rust_entities > 0
            && !reads_rust_entities(id) =>
        {
            rust_unread(
                "Classified entities are declared in Rust services, and none of the offline \
                 checks on this item reads Rust yet. Run `forklaunch score` online for a full \
                 source review.",
            )
        }
        "gov-construction" if evidence.projects == 0 => nothing(no_projects),
        _ => None,
    }
}

/// What a check's finding means for its criterion. Mirrors `onFinding` in the
/// platform's `LOCAL_CHECKS`.
#[derive(PartialEq)]
enum OnFinding {
    /// The item is unmet.
    Fail,
    /// A human should look; the item is neither met nor failed.
    Review,
}

/// Local check id -> (criterion id, what a finding means). Mirrors the
/// platform's `LOCAL_CHECKS` table.
fn criterion_for_check(check: &str) -> Option<(&'static str, OnFinding)> {
    Some(match check {
        "tenant-isolation-wiring" | "tenant-context-half-wired" => {
            ("sec-tenant-isolation", OnFinding::Fail)
        }
        "encryptor-registration" | "tenant-em-wiring" | "better-auth-encryption-context" => {
            ("cmp-encryption-at-rest", OnFinding::Fail)
        }
        "retention-wiring" => ("gov-data-retention", OnFinding::Fail),
        "erasure-wiring" => ("gdpr-erasure", OnFinding::Fail),
        "possible-misclassification" => ("cmp-data-classification", OnFinding::Review),
        // PHI sent to a provider with no BAA is sensitive data not handled
        // according to its sensitivity. (The platform also scores it against
        // `hipaa-baa` when the HIPAA pack applies.)
        "ai-provider-direct" => ("cmp-data-classification", OnFinding::Fail),
        // A managed template that needs credentials hosted instances never get
        // does not run as declared.
        "managed-provider-credentials" => ("gov-construction", OnFinding::Fail),
        // Storage the platform cannot provision, or reaches with the wrong
        // credentials, does not run as declared.
        "object-store-wiring" | "object-store-static-credentials" => {
            ("gov-construction", OnFinding::Fail)
        }
        // Public files and bucket settings changed in code bypass the
        // platform's private, encrypted bucket.
        "object-store-public-access" | "object-store-bucket-managed-in-app" => {
            ("cmp-encryption-at-rest", OnFinding::Fail)
        }
        "presigned-upload-unbounded" => ("gov-construction", OnFinding::Fail),
        // A capability the manifest and the code disagree about is either not
        // provisioned or not callable.
        "capability-wiring" => ("gov-construction", OnFinding::Fail),
        // A managed instance never gets a mail credential; its own provider
        // does not run as declared.
        "email-provider-direct-in-managed" => ("gov-construction", OnFinding::Fail),
        // Protected data shown where it should not be (previews, mail logs).
        "email-protected-data" => ("cmp-data-classification", OnFinding::Fail),
        // A managed service that needs an SMS vendor credential it never gets
        // does not run as declared.
        "sms-provider-direct-in-managed" => ("gov-construction", OnFinding::Fail),
        // Protected plaintext sent over a channel carriers and lock screens
        // read is sensitive data not handled according to its sensitivity.
        "sms-protected-data" => ("cmp-data-classification", OnFinding::Fail),
        // A managed service holding its own WhatsApp credentials does not run
        // as declared; protected data sent over WhatsApp is sensitive data not
        // handled according to its sensitivity.
        "whatsapp-provider-direct-in-managed" => ("gov-construction", OnFinding::Fail),
        "whatsapp-protected-data" => ("cmp-data-classification", OnFinding::Fail),
        // A managed service dialing through its own voice vendor needs
        // credentials hosted instances never get.
        "voice-provider-direct-in-managed" => ("gov-construction", OnFinding::Fail),
        // Protected values in call attributes land in Connect's contact records.
        "voice-protected-data" => ("cmp-data-classification", OnFinding::Fail),
        // A managed payments service that needs a Stripe key a hosted instance
        // never gets does not run as declared.
        "payments-stripe-keys-in-managed" => ("gov-construction", OnFinding::Fail),
        // An unverified webhook is untrusted input acted on.
        "stripe-webhook-unverified" => ("sec-input-validation", OnFinding::Fail),
        // Protected data sent to a vendor with no BAA.
        "payments-protected-data" => ("cmp-data-classification", OnFinding::Fail),
        // A non-public route contract with no auth method lets callers in
        // without a credential.
        "route-auth-missing" => ("sec-authn", OnFinding::Fail),
        // Routes the framework never sees, and files whose routes could not be
        // read: a person confirms them; neither proves nor disproves sign-in.
        "route-outside-framework" | "route-scan-incomplete" => ("sec-authn", OnFinding::Review),
        // A pinned version the advisory database lists as vulnerable or
        // malicious, or no lockfile to pin versions at all.
        "dependency-vulnerable" | "dependency-malicious" | "dependency-lockfile-missing" => {
            ("sec-dependencies", OnFinding::Fail)
        }
        // The versions could not be looked up: not checked, so a person looks.
        "dependency-scan-incomplete" => ("sec-dependencies", OnFinding::Review),
        _ => return None,
    })
}

fn criterion(id: &str) -> Option<&'static Criterion> {
    CRITERIA.iter().find(|c| c.id == id)
}

/// Which rail each deterministic check belongs to: its criterion's rail.
/// Unmapped checks fall back to compliance, as the platform's
/// `CATEGORY_DIMENSIONS` does for unknown categories.
fn rail_for_check(check: &str) -> &'static str {
    criterion_for_check(check)
        .and_then(|(id, _)| criterion(id))
        .map(|c| c.rail)
        .unwrap_or("compliance")
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
        // Health data leaving for a vendor that may not have signed a BAA.
        (Severity::Warning, "ai-provider-direct") => "critical",
        // Anyone on the internet can read the files.
        (Severity::Warning, "object-store-public-access") => "critical",
        // Health data in an email subject: on lock screens and in mail logs.
        (Severity::Warning, "email-protected-data")
            if finding.message.starts_with("health data (phi)") =>
        {
            "critical"
        }
        // Health data texted in the clear; other protected data is high.
        (Severity::Warning, "sms-protected-data") if finding.message.starts_with("health data") => {
            "critical"
        }
        // Health data sent over WhatsApp, which no BAA covers.
        (Severity::Warning, "whatsapp-protected-data")
            if finding
                .message
                .starts_with(crate::compliance::checks::WHATSAPP_PHI_MARKER) =>
        {
            "critical"
        }
        // Health data in call attributes, which Connect keeps in contact records.
        (Severity::Warning, "voice-protected-data") if finding.message.contains("(phi)") => {
            "critical"
        }
        // Health data sent to Stripe, which signs no BAA.
        (Severity::Warning, "payments-protected-data") if finding.subject.starts_with("phi") => {
            "critical"
        }
        // An endpoint anyone can call without signing in.
        (Severity::Warning, "route-auth-missing") => "critical",
        // A malicious release runs its install scripts on every machine that installs it.
        (Severity::Warning, "dependency-malicious") => "critical",
        (Severity::Warning, "dependency-vulnerable") if finding.message.starts_with("CRITICAL") => {
            "critical"
        }
        // The advisory's own rating, as the platform reads it.
        (Severity::Info, "dependency-vulnerable") if finding.message.starts_with("LOW") => "low",
        (Severity::Info, "dependency-vulnerable") => "medium",
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
    /// Platform criterion id the finding bears on, e.g. `sec-tenant-isolation`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) criterion: Option<String>,
}

/// One deterministic check's outcome on an item, as the platform's v3 item
/// `checks` records it (`pass` / `fail` / `review`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardItemCheck {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardItem {
    /// The platform criterion's exact label.
    pub(crate) label: String,
    /// `met` (every check on it passed), `unmet` (a check failed), `pending`
    /// (a check asks for review) or `unassessed` (the checks found nothing to
    /// read; scores zero). Only `unmet` is settled by a check alone:
    /// the offline checks are partial, so a pass is evidence, not proof.
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
    /// Platform criterion id, e.g. `cmp-encryption-at-rest`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) criterion: Option<String>,
    /// The checks that bear on this item and how each came out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) checks: Vec<CardItemCheck>,
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

/// One human-readable line per check, used for finding titles. Checklist
/// items use the criterion label instead (see `CRITERIA`).
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
        "ai-provider-direct" => "Health data reaches AI models only through BAA-covered paths",
        "managed-provider-credentials" => {
            "Managed templates use the platform gateways, not their own credentials"
        }
        "object-store-wiring" => "Object storage is declared and wired",
        "object-store-static-credentials" => {
            "Object storage uses the service's role, not stored keys"
        }
        "object-store-public-access" => "Stored files are private",
        "object-store-bucket-managed-in-app" => "The platform, not app code, configures the bucket",
        "presigned-upload-unbounded" => "Browser uploads are limited in size and type",
        "capability-wiring" => "Platform capabilities are declared and wired",
        "email-provider-direct-in-managed" => "Managed services send email through the platform",
        "email-protected-data" => "Email subjects carry no protected data",
        "sms-provider-direct-in-managed" => {
            "Managed services text through the platform, not an SMS vendor SDK"
        }
        "sms-protected-data" => "Protected data is never sent in a text message",
        "whatsapp-provider-direct-in-managed" => {
            "Managed services send WhatsApp through the platform"
        }
        "whatsapp-protected-data" => "Protected data is never sent over WhatsApp",
        "voice-provider-direct-in-managed" => "Managed services place calls through the platform",
        "voice-protected-data" => "Call attributes carry no protected data",
        "payments-stripe-keys-in-managed" => {
            "Managed payments go through the platform, with no Stripe key in the app"
        }
        "stripe-webhook-unverified" => "Stripe webhooks are verified by signature",
        "payments-protected-data" => "Protected data is not sent to Stripe",
        "route-auth-missing" => "Every non-public route declares how callers sign in",
        "route-outside-framework" => "Routes outside the framework are confirmed open on purpose",
        "route-scan-incomplete" => "Every source file's routes could be read",
        "dependency-vulnerable" => "No pinned package version has a known vulnerability",
        "dependency-malicious" => "No pinned package release is malicious",
        "dependency-lockfile-missing" => "A lockfile pins every dependency version",
        "dependency-scan-incomplete" => "Every pinned version was checked for advisories",
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
        "ai-provider-direct" => {
            "Run `forklaunch infra add <service> models` and call models through the injected \
             ModelGatewayClient (createModelGatewayClient(); the managed gateway offers HIPAA \
             products only BAA-covered models), or confirm the provider has signed a BAA for this use."
        }
        "managed-provider-credentials" => {
            "Send one-time codes through the platform's instance gateway and call models with \
             createModelGatewayClient(); drop the provider credentials from the template."
        }
        "object-store-wiring" => {
            "Run `forklaunch infra add <service> object-store` (or `infra remove`) so the manifest \
             and registrations.ts agree."
        }
        "object-store-static-credentials" => {
            "Build the S3 client with s3ClientConfig({ url, region, accessKeyId, secretAccessKey }) \
             and declare the keys optional; deployed, the task role supplies credentials."
        }
        "object-store-public-access" => {
            "Remove public ACLs and wildcard CORS; serve files with ObjectStore.presignDownload."
        }
        "object-store-bucket-managed-in-app" => {
            "Drop bucket creation, policy and CORS calls; set browser uploads and link lifetimes \
             in the platform's object-store settings."
        }
        "presigned-upload-unbounded" => {
            "Use ObjectStore.presignUpload(key, { contentType, maxBytes }) instead of a presigned PUT."
        }
        "capability-wiring" => {
            "Run `forklaunch infra add <service> <capability>` (or `infra remove`) so the manifest \
             and registrations.ts agree."
        }
        "email-provider-direct-in-managed" => {
            "Run `forklaunch infra add <service> email` and send with the injected EmailClient \
             (createEmailClient()); drop the mail SDK and its SMTP/API credentials."
        }
        "email-protected-data" => {
            "Use a generic subject (\"Your results are ready\") and put the protected detail in the \
             body or behind a signed-in link."
        }
        "sms-provider-direct-in-managed" => {
            "Run `forklaunch infra add <service> sms` and send with the registered SmsClient \
             (createSmsClient); drop the vendor SDK and its keys."
        }
        "sms-protected-data" => {
            "Text a neutral notice or a sign-in link; show the protected value only after sign-in. \
             Never pass a .deanon value in an SMS body."
        }
        "whatsapp-provider-direct-in-managed" => {
            "Run `forklaunch infra add <service> whatsapp` and send with createWhatsAppClient(); \
             drop the Meta token, graph.facebook.com calls and WhatsApp SDKs from the service."
        }
        "whatsapp-protected-data" => {
            "Do not put a .deanon value in a WhatsApp message: WhatsApp is not covered by the AWS \
             BAA and Meta signs none. Send a template that says only that something is waiting \
             in the app."
        }
        "voice-provider-direct-in-managed" => {
            "Run `forklaunch infra add <service> voice` and place calls with createVoiceClient(); \
             drop the Connect/Twilio/Vonage SDK and its credentials from the service."
        }
        "voice-protected-data" => {
            "Pass an identifier (an appointment id) in startOutboundCall attributes and let the \
             contact flow look up what it reads out; never a .deanon value."
        }
        "payments-stripe-keys-in-managed" => {
            "Run `forklaunch infra add <service> payments`: the StripeClient becomes \
             createStripeClient({ Stripe, apiKey }) and the Stripe keys optional, used only outside \
             managed mode."
        }
        "stripe-webhook-unverified" => {
            "Verify each delivery with stripe.webhooks.constructEvent(rawBody, signature, secret) \
             before acting on it; in managed mode receive Stripe events as platform events \
             (verifyPlatformEvent)."
        }
        "payments-protected-data" => {
            "Send Stripe an opaque reference (the record id) in metadata and descriptions, never a \
             .deanon value; look the record up in the app when the event comes back."
        }
        "route-auth-missing" => {
            "Add an auth block to the route's contract (e.g. auth: jwtAuth(ROLES)), or declare it \
             access: 'public' if it is meant to be open."
        }
        "route-outside-framework" => {
            "Declare the route with handlers.* and a contract so the framework applies its access \
             level, auth and validation, or confirm it is meant to be open (health checks, auth \
             library callbacks)."
        }
        "route-scan-incomplete" => {
            "Fix the file's syntax error so its routes can be read, or check them by hand."
        }
        "dependency-vulnerable" => {
            "Upgrade each listed package to its fixed version (or later) and regenerate the \
             lockfile; where no fix exists, replace the package or confirm the vulnerable code \
             path is never reached."
        }
        "dependency-malicious" => {
            "Remove the package, reinstall from a clean lockfile, and rotate any credentials \
             the machines that installed it could reach (developer laptops, CI, build hosts)."
        }
        "dependency-lockfile-missing" => {
            "Commit the package manager's lockfile (pnpm-lock.yaml, bun.lock, package-lock.json, \
             yarn.lock, poetry.lock, uv.lock or Cargo.lock) so every install gets the same, checkable versions."
        }
        "dependency-scan-incomplete" => {
            "Fix any lockfile the finding names, unset FORKLAUNCH_NO_ADVISORY_LOOKUP if it is \
             set, and run `forklaunch compliance audit` with network access so pinned versions \
             can be looked up in the advisory database. Audit unsupported Git/private-registry \
             sources at their pinned revisions separately."
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
    evidence: &LocalEvidence,
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
        // Dependency findings cost points once per check, at the worst rating: one app can
        // pin hundreds of affected versions, and the platform caps the item the same way.
        let mut dependency_penalty: std::collections::BTreeMap<&str, f64> = Default::default();
        for finding in &rail_findings {
            let severity = severity_for(finding);
            // A review finding waits for a person and costs nothing.
            match criterion_for_check(&finding.check).map(|(_, on)| on) {
                Some(OnFinding::Review) => {}
                _ if finding.check.starts_with("dependency-") => {
                    let worst = dependency_penalty
                        .entry(finding.check.as_str())
                        .or_default();
                    *worst = worst.max(penalty_for(severity));
                }
                _ => score -= penalty_for(severity),
            }
            card_findings.push(CardFinding {
                severity: severity.to_string(),
                title: format!("{} ({})", item_label(&finding.check), finding.project),
                detail: finding.message.clone(),
                fix: if finding.check == "route-scan-incomplete"
                    && finding.message.starts_with(UNDECIDED_ROUTES)
                {
                    // The file parsed; its contracts are finished somewhere
                    // the reader does not follow.
                    Some(
                        "Set each contract's access level and auth where it is declared \
                         (.with_access(..).with_auth(..) on the constructor, or #[access(..)] \
                         on a contract endpoint), or check the listed routes by hand."
                            .to_string(),
                    )
                } else {
                    remedy(&finding.check)
                },
                source: "cli".to_string(),
                criterion: criterion_for_check(&finding.check).map(|(id, _)| id.to_string()),
            });
        }
        let score = (score - dependency_penalty.values().sum::<f64>()).max(0.0);

        // One item per criterion on this rail. A check that fired failed (or,
        // for a review check, asks for review); one that never fired passed
        // everywhere it was evaluated.
        let mut items: Vec<CardItem> = Vec::new();
        let mut unassessed = 0usize;
        // Whether an item is unassessed for a reason specific to Rust services.
        let mut rust_unread = false;
        for crit in CRITERIA.iter().filter(|c| c.rail == *rail) {
            let mut checks = Vec::new();
            let mut failed = false;
            let mut review = false;
            let mut detail = None;
            for check in checks_for_criterion(crit.id) {
                let hits: Vec<&&LocalFinding> =
                    rail_findings.iter().filter(|f| f.check == *check).collect();
                let status = match hits.first() {
                    None => "pass",
                    Some(first) => {
                        detail.get_or_insert_with(|| first.message.clone());
                        match criterion_for_check(check).map(|(_, on)| on) {
                            Some(OnFinding::Review) => {
                                review = true;
                                "review"
                            }
                            _ => {
                                failed = true;
                                "fail"
                            }
                        }
                    }
                };
                checks.push(CardItemCheck {
                    id: (*check).to_string(),
                    status: status.to_string(),
                    detail: match hits.first() {
                        None => "No problems found by this check.".to_string(),
                        Some(first) => format!(
                            "{} finding{}: {}{}",
                            hits.len(),
                            if hits.len() > 1 { "s" } else { "" },
                            first.message,
                            if hits.len() > 1 {
                                format!(" (and {} more)", hits.len() - 1)
                            } else {
                                String::new()
                            }
                        ),
                    },
                });
            }
            // No check fired and there was nothing for them to read: that is
            // absence of evidence, and it scores zero rather than passing.
            let nothing_to_read = if failed || review {
                None
            } else {
                missing_evidence(crit.id, evidence)
            };
            // A check that only reads TypeScript says so rather than report a
            // pass on Rust entities it never looked at, whatever the item's
            // status turns out to be.
            let mut not_read = 0usize;
            if ENTITY_CRITERIA.contains(&crit.id) && evidence.rust_entities > 0 {
                let typescript_entities = evidence.entities > evidence.rust_entities;
                for check in checks.iter_mut().filter(|check| check.status == "pass") {
                    if check.id == "encryptor-registration" {
                        if !evidence.rust_unbound.is_empty() {
                            check.status = "skipped".to_string();
                            check.detail = UNBOUND.to_string();
                        }
                    } else if !crate::compliance::checks::RUST_ENTITY_CHECKS
                        .contains(&check.id.as_str())
                    {
                        not_read += 1;
                        if typescript_entities {
                            check.detail = format!(
                                "No problems found in the TypeScript services. {RUST_UNREAD}"
                            );
                        } else {
                            check.status = "skipped".to_string();
                            check.detail = RUST_UNREAD.to_string();
                        }
                    }
                }
            }
            if let Some(unread) = &nothing_to_read {
                unassessed += 1;
                rust_unread |= unread.rust_unread;
                detail = Some(unread.why.clone());
                for check in &mut checks {
                    check.status = "skipped".to_string();
                    check.detail = unread.check.clone();
                }
            } else if !failed && !review {
                // The item is decided by the checks that read Rust.
                if not_read > 0 && detail.is_none() {
                    detail = Some(format!(
                        "Decided for the Rust services by the checks that read Rust ({}). {} other check(s) on this item only read TypeScript and say nothing about the Rust services.",
                        crate::compliance::checks::RUST_ENTITY_CHECKS
                            .iter()
                            .filter(|id| checks.iter().any(|check| check.id == **id))
                            .copied()
                            .collect::<Vec<_>>()
                            .join(", "),
                        not_read
                    ));
                }
            }
            items.push(CardItem {
                label: crit.label.to_string(),
                status: if failed {
                    "unmet"
                } else if review {
                    "pending"
                } else if nothing_to_read.is_some() {
                    "unassessed"
                } else {
                    "met"
                }
                .to_string(),
                detail,
                criterion: Some(crit.id.to_string()),
                checks,
            });
        }

        // Each unassessed item earns nothing: the rail keeps only the share
        // of its score that the assessed items account for.
        let score = if items.is_empty() {
            score
        } else {
            score * (items.len() - unassessed) as f64 / items.len() as f64
        };

        let summary = if unassessed > 0 && rust_unread {
            format!(
                "{} of {} item(s) could not be assessed offline; each says why (nothing to read, or a Rust service the checks cannot decide yet). They score zero.",
                unassessed,
                items.len()
            )
        } else if unassessed > 0 {
            format!(
                "{} of {} item(s) could not be assessed: the offline checks found nothing to read. They score zero.",
                unassessed,
                items.len()
            )
        } else if card_findings.is_empty() {
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
    let unassessed_items = dimensions
        .values()
        .flat_map(|d| d.items.iter())
        .filter(|i| i.status == "unassessed")
        .count();
    let headline = if unassessed_items > 0 {
        format!(
            "{app_name}: {unassessed_items} checklist item(s) could not be assessed offline and score zero; {total} deterministic finding(s) across {module_count} module(s)."
        )
    } else if total == 0 {
        format!(
            "{app_name}: no deterministic compliance or security findings across {module_count} module(s)."
        )
    } else {
        format!(
            "{app_name}: {total} deterministic finding(s) across {module_count} module(s), scored on compliance, security and governance."
        )
    };

    ReportCard {
        schema_version: SCHEMA_VERSION,
        overall,
        headline,
        caveat: "Deterministic checks only. Compliance, security and governance are scored \
                 from static analysis of the checklist items those checks cover; scalability \
                 and observability need judgement a source read cannot supply and are left \
                 unassessed rather than scored zero. The overall score is weighted across the \
                 scored rails only. A checklist item whose checks found nothing to read is \
                 unassessed and scores zero. All five rails need an agent's judgement, which the studio \
                 surface provides."
            .to_string(),
        phase: "audit".to_string(),
        step: "audit".to_string(),
        frameworks: Vec::new(),
        dimensions,
        generated_at,
    }
}

/// Every local check, so the checklist shows passes as well as failures.
const ALL_CHECKS: &[&str] = crate::compliance::checks::LOCAL_CHECK_IDS;

/// The local checks that bear on a criterion.
fn checks_for_criterion(id: &str) -> Vec<&'static str> {
    ALL_CHECKS
        .iter()
        .copied()
        .filter(|check| criterion_for_check(check).is_some_and(|(c, _)| c == id))
        .collect()
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

    /// A workspace with something for every check to read.
    fn read() -> LocalEvidence {
        LocalEvidence {
            routes: 4,
            entities: 2,
            projects: 1,
            ..LocalEvidence::default()
        }
    }

    /// An app with no ForkLaunch code: nothing fired because nothing was
    /// read. That must score zero, never full marks.
    #[test]
    fn nothing_to_read_scores_zero_not_full_marks() {
        let card = build_local_report_card("loandesk", 3, &[], &LocalEvidence::default(), at());
        for (rail, id) in [
            ("security", "sec-authn"),
            ("security", "sec-tenant-isolation"),
            ("security", "sec-input-validation"),
            ("compliance", "cmp-encryption-at-rest"),
            ("compliance", "cmp-data-classification"),
            ("governance", "gov-data-retention"),
            ("governance", "gdpr-erasure"),
            ("governance", "gov-construction"),
        ] {
            let item = card.dimensions[rail]
                .items
                .iter()
                .find(|i| i.criterion.as_deref() == Some(id))
                .unwrap();
            assert_eq!(item.status, "unassessed", "{id}");
            assert!(item.detail.is_some(), "{id} must say why");
            assert!(item.checks.iter().all(|c| c.status == "skipped"), "{id}");
        }
        assert_eq!(card.dimensions["compliance"].score, 0);
        assert_eq!(card.dimensions["governance"].score, 0);
        // Dependencies are judged from lockfiles, so that one item still counts.
        assert_eq!(card.dimensions["security"].score, 25);
        assert!(card.overall < 15, "overall was {}", card.overall);
        assert!(
            card.headline.contains("could not be assessed"),
            "{}",
            card.headline
        );
    }

    /// Evidence is per criterion: routes without entities settles sign-in but
    /// not encryption.
    #[test]
    fn evidence_is_judged_per_criterion() {
        let evidence = LocalEvidence {
            routes: 3,
            entities: 0,
            projects: 1,
            ..LocalEvidence::default()
        };
        let card = build_local_report_card("demo", 1, &[], &evidence, at());
        let status = |rail: &str, id: &str| {
            card.dimensions[rail]
                .items
                .iter()
                .find(|i| i.criterion.as_deref() == Some(id))
                .unwrap()
                .status
                .clone()
        };
        assert_eq!(status("security", "sec-authn"), "met");
        assert_eq!(status("governance", "gov-construction"), "met");
        assert_eq!(status("compliance", "cmp-encryption-at-rest"), "unassessed");
        assert_eq!(card.dimensions["compliance"].score, 0);
    }

    fn item<'c>(card: &'c ReportCard, rail: &str, id: &str) -> &'c CardItem {
        card.dimensions[rail]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some(id))
            .unwrap()
    }

    /// A Rust service's routes and classifications are read, so the items
    /// those decide are assessed. The entity checks that only look for
    /// TypeScript wiring have decided nothing about it: their items stay
    /// unassessed and say the Rust check does not exist yet.
    #[test]
    fn a_rust_service_is_met_only_where_a_check_read_rust() {
        let evidence = LocalEvidence {
            routes: 6,
            rust_routes: 6,
            rust_contract_routes: 6,
            entities: 3,
            rust_entities: 3,
            projects: 1,
            ..LocalEvidence::default()
        };
        let card = build_local_report_card("notes", 1, &[], &evidence, at());
        for (rail, id) in [
            ("security", "sec-authn"),
            ("security", "sec-input-validation"),
            ("compliance", "cmp-encryption-at-rest"),
            ("compliance", "cmp-data-classification"),
            ("governance", "gov-construction"),
        ] {
            assert_eq!(item(&card, rail, id).status, "met", "{id}");
        }
        for (rail, id) in [
            ("security", "sec-tenant-isolation"),
            ("governance", "gov-data-retention"),
            ("governance", "gdpr-erasure"),
        ] {
            let unread = item(&card, rail, id);
            assert_eq!(unread.status, "unassessed", "{id}");
            let why = unread.detail.as_deref().unwrap();
            assert!(
                why.contains("Rust") && why.contains("does not exist yet"),
                "{id}: {why}"
            );
            assert!(
                unread
                    .checks
                    .iter()
                    .all(|c| c.status == "skipped" && c.detail == RUST_UNREAD),
                "{id}"
            );
        }
        // On a decided item, a TypeScript-only check is skipped, not passed.
        let classification = item(&card, "compliance", "cmp-data-classification");
        let status = |id: &str| {
            classification
                .checks
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .status
                .as_str()
        };
        assert_eq!(status("possible-misclassification"), "pass");
        assert_eq!(status("sms-protected-data"), "skipped");
        assert!(
            classification
                .detail
                .as_deref()
                .unwrap()
                .contains("possible-misclassification"),
            "the item names the check that decided it"
        );
        let encryption = item(&card, "compliance", "cmp-encryption-at-rest");
        assert_eq!(
            encryption
                .checks
                .iter()
                .filter(|c| c.status == "pass")
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["encryptor-registration"]
        );
        assert_eq!(card.dimensions["governance"].score, 33);
        assert!(card.dimensions["governance"].summary.contains("Rust"));
    }

    /// TypeScript entities that pass do not vouch for Rust entities beside them.
    #[test]
    fn a_mixed_app_is_not_met_on_the_strength_of_its_typescript_half() {
        let evidence = LocalEvidence {
            entities: 5,
            rust_entities: 2,
            ..read()
        };
        let card = build_local_report_card("mixed", 2, &[], &evidence, at());
        assert_eq!(
            item(&card, "governance", "gov-data-retention").status,
            "unassessed"
        );
        assert_eq!(
            item(&card, "compliance", "cmp-encryption-at-rest").status,
            "met"
        );
        let better_auth = item(&card, "compliance", "cmp-encryption-at-rest")
            .checks
            .iter()
            .find(|c| c.id == "better-auth-encryption-context")
            .unwrap();
        assert_eq!(better_auth.status, "pass");
        assert!(better_auth.detail.contains("TypeScript services"));
        // A failure found in the TypeScript half still decides the item.
        let failed = build_local_report_card(
            "mixed",
            2,
            &[finding("retention-wiring", Severity::Warning)],
            &evidence,
            at(),
        );
        assert_eq!(
            item(&failed, "governance", "gov-data-retention").status,
            "unmet"
        );
    }

    /// A classified Rust type with no protected binding is neither confirmed
    /// encrypted nor shown to be plaintext.
    #[test]
    fn an_unbound_rust_classification_leaves_encryption_undecided() {
        let evidence = LocalEvidence {
            entities: 2,
            rust_entities: 2,
            rust_unbound: vec!["SealedEvent.ciphertext (insights/queue.rs:15)".to_string()],
            ..read()
        };
        let card = build_local_report_card("notes", 1, &[], &evidence, at());
        let encryption = item(&card, "compliance", "cmp-encryption-at-rest");
        assert_eq!(encryption.status, "unassessed");
        assert!(
            encryption
                .detail
                .as_deref()
                .unwrap()
                .contains("SealedEvent.ciphertext (insights/queue.rs:15)")
        );
        assert_eq!(
            item(&card, "compliance", "cmp-data-classification").status,
            "met"
        );
    }

    /// A file that parsed but holds routes the reader could not decide is
    /// not told to fix a syntax error.
    #[test]
    fn undecided_routes_get_their_own_remedy() {
        let mut undecided = finding("route-scan-incomplete", Severity::Warning);
        undecided.message =
            format!("{UNDECIDED_ROUTES}2 route(s) the offline reader could not decide");
        let unparsed = finding("route-scan-incomplete", Severity::Warning);
        let card = build_local_report_card("demo", 1, &[undecided, unparsed], &read(), at());
        let fixes: Vec<_> = card.dimensions["security"]
            .findings
            .iter()
            .map(|f| f.fix.clone().unwrap())
            .collect();
        assert!(fixes[0].contains(".with_access(..)") && !fixes[0].contains("syntax"));
        assert!(fixes[1].contains("syntax error"));
        assert_eq!(item(&card, "security", "sec-authn").status, "pending");
    }

    /// Raw Rust routes are something to read for sign-in, but none of them
    /// shows that input is validated.
    #[test]
    fn raw_rust_routes_do_not_show_input_validation() {
        let evidence = LocalEvidence {
            routes: 14,
            rust_routes: 14,
            ..LocalEvidence::default()
        };
        let outside = finding("route-outside-framework", Severity::Info);
        let card = build_local_report_card("axum", 1, &[outside], &evidence, at());
        assert_eq!(item(&card, "security", "sec-authn").status, "pending");
        let validation = item(&card, "security", "sec-input-validation");
        assert_eq!(validation.status, "unassessed");
        assert!(
            validation
                .detail
                .as_deref()
                .unwrap()
                .contains("outside ForkLaunch contracts")
        );
        // One contract route among them is evidence the framework validates.
        let with_contract = LocalEvidence {
            rust_contract_routes: 1,
            ..evidence
        };
        let card = build_local_report_card("axum", 1, &[], &with_contract, at());
        assert_eq!(
            item(&card, "security", "sec-input-validation").status,
            "met"
        );
    }

    fn at() -> String {
        "2026-01-01T00:00:00Z".to_string()
    }

    #[test]
    fn a_clean_workspace_scores_full_marks_on_the_rails_it_can_judge() {
        let card = build_local_report_card("demo", 3, &[], &read(), at());
        assert_eq!(card.overall, 100);
        assert_eq!(card.dimensions["compliance"].score, 100);
        assert_eq!(card.dimensions["security"].score, 100);
        assert_eq!(card.dimensions["governance"].score, 100);
        assert_eq!(card.dimensions["governance"].pending, None);
    }

    /// Every item carries the platform criterion id and its exact label, so
    /// the offline card and the website's "Checked" half list the same lines.
    #[test]
    fn items_use_platform_criterion_ids_and_exact_labels() {
        let card = build_local_report_card("demo", 1, &[], &read(), at());
        let expected = [
            (
                "compliance",
                "cmp-encryption-at-rest",
                "Sensitive fields are encrypted at rest",
            ),
            (
                "compliance",
                "cmp-data-classification",
                "Sensitive data is identified and handled according to its sensitivity",
            ),
            (
                "security",
                "sec-tenant-isolation",
                "One customer cannot see or change another customer\u{2019}s data",
            ),
            (
                "security",
                "sec-authn",
                "Every endpoint requires sign-in unless it is explicitly public",
            ),
            (
                "security",
                "sec-input-validation",
                "Incoming data is validated before it is trusted",
            ),
            (
                "security",
                "sec-dependencies",
                "No installed dependency has a known vulnerability or is malicious",
            ),
            (
                "governance",
                "gov-data-retention",
                "How long data is kept, and how it is deleted, is defined",
            ),
            (
                "governance",
                "gdpr-erasure",
                "Users can request data export and erasure",
            ),
            (
                "governance",
                "gov-construction",
                "The app builds cleanly to its declared parameters (construction check passes)",
            ),
        ];
        let total: usize = card.dimensions.values().map(|d| d.items.len()).sum();
        assert_eq!(total, expected.len());
        for (rail, id, label) in expected {
            let item = card.dimensions[rail]
                .items
                .iter()
                .find(|i| i.criterion.as_deref() == Some(id))
                .unwrap_or_else(|| panic!("{rail} has no {id} item"));
            assert_eq!(item.label, label);
        }
        let json = serde_json::to_value(&card).unwrap();
        assert_eq!(
            json["dimensions"]["security"]["items"][0]["criterion"],
            "sec-tenant-isolation"
        );
        assert!(
            json["dimensions"]["security"]["items"][0]["label"]
                .as_str()
                .unwrap()
                .contains('\u{2019}'),
            "the label must keep the typographic apostrophe"
        );
    }

    #[test]
    fn an_endpoint_without_sign_in_is_critical_and_outside_routes_are_for_review() {
        let missing = finding("route-auth-missing", Severity::Warning);
        assert_eq!(severity_for(&missing), "critical");
        assert!(matches!(
            criterion_for_check("route-outside-framework"),
            Some(("sec-authn", OnFinding::Review))
        ));
        assert!(
            remedy("route-auth-missing").is_some() && remedy("route-outside-framework").is_some()
        );
        assert_eq!(rail_for_check("route-auth-missing"), "security");
    }

    #[test]
    fn every_local_check_maps_to_its_platform_criterion() {
        for (check, id) in [
            ("tenant-isolation-wiring", "sec-tenant-isolation"),
            ("tenant-context-half-wired", "sec-tenant-isolation"),
            ("encryptor-registration", "cmp-encryption-at-rest"),
            ("tenant-em-wiring", "cmp-encryption-at-rest"),
            ("better-auth-encryption-context", "cmp-encryption-at-rest"),
            ("retention-wiring", "gov-data-retention"),
            ("erasure-wiring", "gdpr-erasure"),
            ("possible-misclassification", "cmp-data-classification"),
            ("payments-stripe-keys-in-managed", "gov-construction"),
            ("stripe-webhook-unverified", "sec-input-validation"),
            ("payments-protected-data", "cmp-data-classification"),
            ("route-auth-missing", "sec-authn"),
            ("route-outside-framework", "sec-authn"),
            ("route-scan-incomplete", "sec-authn"),
            ("dependency-vulnerable", "sec-dependencies"),
            ("dependency-malicious", "sec-dependencies"),
            ("dependency-lockfile-missing", "sec-dependencies"),
            ("dependency-scan-incomplete", "sec-dependencies"),
        ] {
            assert_eq!(
                criterion_for_check(check).map(|(c, _)| c),
                Some(id),
                "{check}"
            );
            assert!(
                ALL_CHECKS.contains(&check),
                "{check} missing from ALL_CHECKS"
            );
        }
    }

    #[test]
    fn a_failed_check_marks_its_criterion_unmet_and_names_itself() {
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("tenant-em-wiring", Severity::Warning)],
            &read(),
            at(),
        );
        let item = card.dimensions["compliance"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("cmp-encryption-at-rest"))
            .unwrap();
        assert_eq!(item.status, "unmet");
        let em = item
            .checks
            .iter()
            .find(|c| c.id == "tenant-em-wiring")
            .unwrap();
        assert_eq!(em.status, "fail");
        assert!(
            item.checks
                .iter()
                .filter(|c| c.id != "tenant-em-wiring")
                .all(|c| c.status == "pass")
        );
        assert_eq!(
            card.dimensions["compliance"].findings[0]
                .criterion
                .as_deref(),
            Some("cmp-encryption-at-rest")
        );
    }

    /// Misclassification is a review, not a failure: the item is left pending.
    #[test]
    fn possible_misclassification_is_a_review_not_a_failure() {
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("possible-misclassification", Severity::Info)],
            &read(),
            at(),
        );
        let item = card.dimensions["compliance"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("cmp-data-classification"))
            .unwrap();
        assert_eq!(item.status, "pending");
        assert_eq!(item.checks[0].status, "review");
    }

    #[test]
    fn malicious_and_critical_dependencies_are_critical() {
        assert_eq!(
            severity_for(&finding("dependency-malicious", Severity::Warning)),
            "critical"
        );
        let mut critical = finding("dependency-vulnerable", Severity::Warning);
        critical.message = "CRITICAL GHSA-xxxx: prototype pollution".to_string();
        assert_eq!(severity_for(&critical), "critical");
        let mut high = finding("dependency-vulnerable", Severity::Warning);
        high.message = "HIGH GHSA-yyyy: ReDoS".to_string();
        assert_eq!(severity_for(&high), "high");
        let mut moderate = finding("dependency-vulnerable", Severity::Info);
        moderate.message = "MODERATE GHSA-zzzz: open redirect".to_string();
        assert_eq!(severity_for(&moderate), "medium");
        moderate.message = "LOW GHSA-zzzz: info leak".to_string();
        assert_eq!(severity_for(&moderate), "low");
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("dependency-scan-incomplete", Severity::Warning)],
            &read(),
            at(),
        );
        let item = card.dimensions["security"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("sec-dependencies"))
            .unwrap();
        assert_eq!(item.status, "pending");
        assert_eq!(card.dimensions["security"].score, 100);
    }

    #[test]
    fn many_vulnerable_versions_cost_the_worst_rating_once() {
        let mut findings = Vec::new();
        for i in 0..20 {
            let mut f = finding("dependency-vulnerable", Severity::Warning);
            f.message = format!("HIGH GHSA-{i}: x");
            findings.push(f);
        }
        let mut critical = finding("dependency-vulnerable", Severity::Warning);
        critical.message = "CRITICAL GHSA-c: y".to_string();
        findings.push(critical);
        let card = build_local_report_card("demo", 1, &findings, &read(), at());
        assert_eq!(card.dimensions["security"].score, 70);
    }

    /// An unreadable file leaves sign-in for review and costs no points.
    #[test]
    fn an_incomplete_route_scan_is_reviewed_without_a_deduction() {
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("route-scan-incomplete", Severity::Warning)],
            &read(),
            at(),
        );
        let security = &card.dimensions["security"];
        assert_eq!(security.score, 100);
        let item = security
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("sec-authn"))
            .unwrap();
        assert_eq!(item.status, "pending");
        let scan = item
            .checks
            .iter()
            .find(|c| c.id == "route-scan-incomplete")
            .unwrap();
        assert_eq!(scan.status, "review");
    }

    #[test]
    fn unassessed_rails_are_pending_not_zero_scores() {
        // Scoring them 0 would cap every card at 50 and read as failure rather
        // than absence -- the distinction the `pending` flag exists to make.
        let card = build_local_report_card("demo", 1, &[], &read(), at());
        for rail in ["scalability", "observability"] {
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
            &read(),
            at(),
        );
        // Same rail (sec-tenant-isolation), ordinary severity.
        let ordinary = build_local_report_card(
            "demo",
            1,
            &[finding("tenant-isolation-wiring", Severity::Warning)],
            &read(),
            at(),
        );

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
                finding("tenant-isolation-wiring", Severity::Warning),
                finding("tenant-em-wiring", Severity::Warning),
                finding("retention-wiring", Severity::Warning),
            ],
            &read(),
            at(),
        );
        // Rails follow the criterion: tenant-em-wiring binds the encryption
        // tenant, so it bears on encryption at rest (compliance), not isolation.
        assert_eq!(card.dimensions["security"].findings.len(), 1);
        assert_eq!(card.dimensions["compliance"].findings.len(), 1);
        assert_eq!(card.dimensions["governance"].findings.len(), 1);
    }

    #[test]
    fn a_rail_never_scores_below_zero() {
        let many: Vec<LocalFinding> = (0..20)
            .map(|_| finding("tenant-context-half-wired", Severity::Warning))
            .collect();
        let card = build_local_report_card("demo", 1, &many, &read(), at());
        assert_eq!(card.dimensions["security"].score, 0);
    }

    #[test]
    fn info_findings_are_reported_without_costing_points() {
        let card = build_local_report_card(
            "demo",
            1,
            &[finding("possible-misclassification", Severity::Info)],
            &read(),
            at(),
        );
        assert_eq!(card.dimensions["compliance"].score, 100);
        assert_eq!(card.dimensions["compliance"].findings.len(), 1);
        assert_eq!(card.dimensions["compliance"].findings[0].severity, "info");
    }

    #[test]
    fn passing_checks_are_listed_as_met_so_the_checklist_shows_work_done() {
        let card = build_local_report_card("demo", 1, &[], &read(), at());
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
            &read(),
            at(),
        );
        let f = &card.dimensions["security"].findings[0];
        assert_eq!(f.source, "cli");
        assert!(
            f.fix
                .as_ref()
                .is_some_and(|s| s.contains("wrapEmWithTenantContext"))
        );
    }

    #[test]
    fn the_card_declares_the_contract_version_and_audit_phase() {
        let card = build_local_report_card("demo", 1, &[], &read(), at());
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

#[cfg(test)]
mod wiring_score_tests {
    use super::*;
    use crate::compliance::checks::{LocalFinding, Severity};

    fn finding(check: &str) -> LocalFinding {
        LocalFinding {
            severity: Severity::Warning,
            project: "svc".to_string(),
            check: check.to_string(),
            subject: "x".to_string(),
            message: "m".to_string(),
        }
    }

    #[test]
    fn protected_data_to_stripe_is_critical_only_with_phi() {
        let mut phi = finding("payments-protected-data");
        phi.subject = "phi: customers.create".to_string();
        assert_eq!(severity_for(&phi), "critical");
        let mut pii = finding("payments-protected-data");
        pii.subject = "customers.create".to_string();
        assert_eq!(severity_for(&pii), "high");
        assert_eq!(severity_for(&finding("stripe-webhook-unverified")), "high");
        assert!(
            remedy("payments-stripe-keys-in-managed")
                .unwrap()
                .contains("infra add")
        );
    }

    #[test]
    fn a_direct_ai_provider_on_phi_costs_critical_points_on_compliance() {
        let clean =
            build_local_report_card("app", 1, &[], &LocalEvidence::everything(), "t".to_string());
        let card = build_local_report_card(
            "app",
            1,
            &[finding("ai-provider-direct")],
            &LocalEvidence::everything(),
            "t".to_string(),
        );
        let before = clean.dimensions["compliance"].score;
        let after = card.dimensions["compliance"].score;
        assert_eq!(before - after, 30, "critical = 30 points");
        let item = card.dimensions["compliance"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("cmp-data-classification"))
            .unwrap();
        assert_eq!(item.status, "unmet");
        assert!(card.overall < clean.overall);
    }

    #[test]
    fn email_checks_map_to_their_criteria_and_phi_subjects_are_critical() {
        let mut phi = finding("email-protected-data");
        phi.message = "health data (phi): an email subject carries …".to_string();
        assert_eq!(severity_for(&phi), "critical");
        assert_eq!(severity_for(&finding("email-protected-data")), "high");
        assert_eq!(
            severity_for(&finding("email-provider-direct-in-managed")),
            "high"
        );
        let card = build_local_report_card(
            "app",
            1,
            &[finding("email-provider-direct-in-managed")],
            &LocalEvidence::everything(),
            "t".to_string(),
        );
        assert_eq!(card.dimensions["governance"].score, 85);
        let card = build_local_report_card(
            "app",
            1,
            &[phi],
            &LocalEvidence::everything(),
            "t".to_string(),
        );
        let item = card.dimensions["compliance"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("cmp-data-classification"))
            .unwrap();
        assert_eq!(item.status, "unmet");
        assert!(remedy("email-protected-data").is_some());
        assert_ne!(
            item_label("email-provider-direct-in-managed"),
            "Deterministic check"
        );
    }

    #[test]
    fn whatsapp_protected_data_is_critical_only_on_phi() {
        let mut phi = finding("whatsapp-protected-data");
        phi.message = format!(
            "{} and a compliant field's plaintext (.deanon) is sent over WhatsApp",
            crate::compliance::checks::WHATSAPP_PHI_MARKER
        );
        assert_eq!(severity_for(&phi), "critical");
        assert_eq!(severity_for(&finding("whatsapp-protected-data")), "high");
        assert_eq!(
            severity_for(&finding("whatsapp-provider-direct-in-managed")),
            "high"
        );
        let card = build_local_report_card(
            "app",
            1,
            &[phi],
            &LocalEvidence::everything(),
            "t".to_string(),
        );
        let item = card.dimensions["compliance"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("cmp-data-classification"))
            .unwrap();
        assert_eq!(item.status, "unmet");
        assert_eq!(
            card.dimensions["compliance"].score, 70,
            "critical = 30 points"
        );
        let card = build_local_report_card(
            "app",
            1,
            &[finding("whatsapp-provider-direct-in-managed")],
            &LocalEvidence::everything(),
            "t".to_string(),
        );
        assert_eq!(card.dimensions["governance"].score, 85, "high = 15 points");
        assert!(remedy("whatsapp-protected-data").is_some());
    }

    #[test]
    fn managed_provider_credentials_fail_construction_on_governance() {
        let card = build_local_report_card(
            "app",
            1,
            &[finding("managed-provider-credentials")],
            &LocalEvidence::everything(),
            "t".to_string(),
        );
        let item = card.dimensions["governance"]
            .items
            .iter()
            .find(|i| i.criterion.as_deref() == Some("gov-construction"))
            .unwrap();
        assert_eq!(item.status, "unmet");
        assert_eq!(card.dimensions["governance"].score, 85, "high = 15 points");
    }
}

#[cfg(test)]
mod voice_tests {
    use super::*;

    fn finding(check: &str, message: &str) -> LocalFinding {
        LocalFinding {
            severity: Severity::Warning,
            project: "clinic".to_string(),
            check: check.to_string(),
            subject: ".deanon".to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn voice_checks_score_high_and_critical_with_phi() {
        assert_eq!(
            severity_for(&finding("voice-protected-data", "attributes")),
            "high"
        );
        assert_eq!(
            severity_for(&finding(
                "voice-protected-data",
                "attributes and this service holds health data (phi)"
            )),
            "critical"
        );
        assert_eq!(
            severity_for(&finding("voice-provider-direct-in-managed", "x")),
            "high"
        );
        for check in ["voice-protected-data", "voice-provider-direct-in-managed"] {
            let (criterion_id, _) = criterion_for_check(check).expect("mapped");
            assert!(
                criterion(criterion_id).is_some(),
                "{check} -> {criterion_id}"
            );
            assert_ne!(item_label(check), "Deterministic check");
            assert!(remedy(check).is_some());
        }
    }
}
