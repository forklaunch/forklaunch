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

/// Every check `run_local_checks` performs. Reported in the audit JSON as
/// `localChecks`, so a consumer can tell a check that ran and found nothing
/// (a pass) from one this CLI version does not have.
pub(crate) const LOCAL_CHECK_IDS: &[&str] = &[
    "encryptor-registration",
    "tenant-em-wiring",
    "better-auth-encryption-context",
    "possible-misclassification",
    "tenant-isolation-wiring",
    "tenant-context-half-wired",
    "retention-wiring",
    "erasure-wiring",
    "ai-provider-direct",
    "managed-provider-credentials",
    "object-store-wiring",
    "object-store-static-credentials",
    "object-store-public-access",
    "object-store-bucket-managed-in-app",
    "presigned-upload-unbounded",
    "capability-wiring",
    "email-provider-direct-in-managed",
    "email-protected-data",
];

/// AI model provider SDKs a service might call directly.
const AI_PROVIDER_PACKAGES: &[&str] = &[
    "openai",
    "@anthropic-ai/sdk",
    "@azure/openai",
    "@aws-sdk/client-bedrock-runtime",
    "@google/generative-ai",
    "@google/genai",
    "cohere-ai",
    "@mistralai/mistralai",
    "groq-sdk",
    "together-ai",
];

/// Vercel AI SDK provider packages are `@ai-sdk/<provider>`.
const AI_SDK_PROVIDER_PREFIX: &str = "@ai-sdk/";

/// Credentials for AI model providers.
const AI_PROVIDER_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "AWS_BEARER_TOKEN_BEDROCK",
    "GEMINI_API_KEY",
    "GOOGLE_GENERATIVE_AI_API_KEY",
    "COHERE_API_KEY",
    "MISTRAL_API_KEY",
    "GROQ_API_KEY",
];

/// Credentials for messaging providers a managed platform supplies itself.
const MESSAGING_PROVIDER_KEYS: &[&str] = &[
    "TWILIO_ACCOUNT_SID",
    "TWILIO_AUTH_TOKEN",
    "SENDGRID_API_KEY",
    "MAILGUN_API_KEY",
    "POSTMARK_SERVER_TOKEN",
];

/// Signals that a service is written to run as a managed instance: it reads
/// the instance-gateway contract the platform injects.
const MANAGED_INSTANCE_MARKERS: &[&str] = &["INSTANCE_HMAC_KEY", "PLATFORM_GATEWAY_URL"];

/// The module specifiers a source imports or requires, in order of appearance.
fn imported_modules(sources: &str) -> Vec<String> {
    static IMPORTS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = IMPORTS.get_or_init(|| {
        regex::Regex::new(
            r#"(?:\bfrom\s*|\bimport\s*\(\s*|\brequire\s*\(\s*|^\s*import\s+)['"]([^'"]+)['"]"#,
        )
        .expect("import pattern")
    });
    re.captures_iter(sources)
        .map(|c| c[1].to_string())
        .collect()
}

/// AI providers the sources call directly: provider SDK imports and provider
/// key reads. Sorted and de-duplicated.
pub(crate) fn direct_ai_providers(sources: &str) -> Vec<String> {
    let mut found: Vec<String> = imported_modules(sources)
        .into_iter()
        .filter(|module| {
            AI_PROVIDER_PACKAGES.contains(&module.as_str())
                || module.starts_with(AI_SDK_PROVIDER_PREFIX)
        })
        .collect();
    found.extend(
        AI_PROVIDER_KEYS
            .iter()
            .filter(|key| sources.contains(**key))
            .map(|key| key.to_string()),
    );
    found.sort();
    found.dedup();
    found
}

/// Provider credentials the sources read, for a managed template.
pub(crate) fn provider_credentials_read(sources: &str) -> Vec<String> {
    let mut found: Vec<String> = MESSAGING_PROVIDER_KEYS
        .iter()
        .chain(AI_PROVIDER_KEYS.iter())
        .filter(|key| sources.contains(**key))
        .map(|key| key.to_string())
        .collect();
    found.sort();
    found
}

/// Email provider SDKs. A managed instance sends email through the platform
/// (`createEmailClient`), which holds the SES identity and the suppression
/// list; a provider SDK in the instance needs a credential it never gets.
const EMAIL_PROVIDER_PACKAGES: &[&str] = &[
    "nodemailer",
    "@aws-sdk/client-ses",
    "@aws-sdk/client-sesv2",
    "@sendgrid/mail",
    "postmark",
    "mailgun.js",
    "mailgun-js",
];

/// Whether a credential name belongs to an email provider (SMTP, SendGrid,
/// Postmark, Mailgun). `email-provider-direct-in-managed` reports these, so
/// `managed-provider-credentials` leaves them out rather than report twice.
pub(crate) fn is_email_credential(key: &str) -> bool {
    key.starts_with("SMTP_")
        || key == "SENDGRID_API_KEY"
        || key.starts_with("POSTMARK_")
        || key.starts_with("MAILGUN_")
}

/// Email providers the sources use directly: provider SDK imports (including
/// subpaths) and provider credential reads. Sorted and de-duplicated.
pub(crate) fn direct_email_providers(sources: &str) -> Vec<String> {
    static KEYS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let keys = KEYS.get_or_init(|| {
        regex::Regex::new(r"\b(?:SMTP_[A-Z0-9_]+|SENDGRID_API_KEY|POSTMARK_[A-Z0-9_]+|MAILGUN_[A-Z0-9_]+)\b")
            .expect("email credential pattern")
    });
    let mut found: Vec<String> = imported_modules(sources)
        .into_iter()
        .filter(|module| {
            EMAIL_PROVIDER_PACKAGES.iter().any(|p| {
                module == p || module.starts_with(&format!("{p}/"))
            })
        })
        .collect();
    found.extend(keys.find_iter(sources).map(|m| m.as_str().to_string()));
    found.sort();
    found.dedup();
    found
}

/// The text of the balanced `{ … }` starting at `open` (a `{`), or to the end.
fn balanced_object(text: &str, open: usize) -> &str {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return &text[open..=i];
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    &text[open..]
}

/// The value expression after a `key:` at `start`: up to the next top-level
/// comma or closing bracket (template literals and nested calls kept whole).
fn property_value(text: &str, start: usize) -> &str {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' if depth == 0 => return &text[start..i],
                b'}' | b')' | b']' => depth -= 1,
                b',' | b';' if depth == 0 => return &text[start..i],
                _ => {}
            },
        }
        i += 1;
    }
    &text[start..]
}

/// Email sends whose subject carries a `.deanon` value, as the subject
/// expressions found.
///
/// Conservative on purpose: only an object literal passed straight to an
/// email send is read — `sendMail(…)`/`sendEmail(…)`, `.send(…)` on a
/// receiver whose name mentions mail (`EmailClient.send`, `mailer.send`,
/// `sgMail.send`), or `new SendEmailCommand(…)` — and only its own `subject`
/// (SES's `Subject: { Data }` included). A body may carry what the recipient
/// is entitled to read; a subject is shown in notification previews and mail
/// logs, so plaintext protected data there is the finding.
pub(crate) fn protected_data_in_email_subjects(sources: &str) -> Vec<String> {
    static CALLS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SUBJECT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let calls = CALLS.get_or_init(|| {
        regex::Regex::new(
            r"(?:\b([A-Za-z_$][\w$]*(?:\s*\.\s*[A-Za-z_$][\w$]*)*)\s*\.\s*(send|sendMail|sendEmail)|\bnew\s+(SendEmailCommand))\s*\(\s*\{",
        )
        .expect("email send pattern")
    });
    let subject = SUBJECT.get_or_init(|| {
        regex::Regex::new(r#"(?:^|[{,\s])['"]?(?i:subject)['"]?\s*:"#).expect("subject pattern")
    });
    let mut found = Vec::new();
    for call in calls.captures_iter(sources) {
        let receiver = call.get(1).map(|m| m.as_str()).unwrap_or("");
        let method = call.get(2).map(|m| m.as_str()).unwrap_or("");
        let is_email = call.get(3).is_some()
            || method == "sendMail"
            || method == "sendEmail"
            || receiver.to_ascii_lowercase().contains("mail");
        if !is_email {
            continue;
        }
        let whole = call.get(0).unwrap();
        let object = balanced_object(sources, whole.end() - 1);
        for m in subject.find_iter(object) {
            let value = property_value(object, m.end()).trim();
            if value.contains(".deanon") {
                found.push(value.split_whitespace().collect::<Vec<_>>().join(" "));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Whether a project is written to run as a managed instance: it reads the
/// instance-gateway contract, or carries the relay the managed installer adds.
pub(crate) fn is_managed_instance(project_path: &Path, sources: &str) -> bool {
    MANAGED_INSTANCE_MARKERS
        .iter()
        .any(|marker| sources.contains(marker))
        || project_path
            .join("api/controllers/relay.controller.ts")
            .exists()
}

/// Object-store capability as the manifest declares it, per project name.
/// None when no manifest is found above the modules directory.
fn declared_object_stores(modules_path: &Path) -> Option<std::collections::HashMap<String, bool>> {
    let mut dir = modules_path.canonicalize().ok()?;
    loop {
        let manifest = dir.join(".forklaunch").join("manifest.toml");
        if manifest.exists() {
            let text = fs::read_to_string(manifest).ok()?;
            let value: toml::Value = toml::from_str(&text).ok()?;
            let projects = value.get("projects")?.as_array()?;
            return Some(
                projects
                    .iter()
                    .filter_map(|p| {
                        let name = p.get("name")?.as_str()?.to_string();
                        let declared = p
                            .get("resources")
                            .and_then(|r| r.get("object_store"))
                            .and_then(|o| o.as_str())
                            .is_some();
                        Some((name, declared))
                    })
                    .collect(),
            );
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Platform-held capabilities (payments, email, …) the manifest declares, per
/// project name. None when no manifest is found above the modules directory.
fn declared_capabilities(modules_path: &Path) -> Option<std::collections::HashMap<String, Vec<String>>> {
    let mut dir = modules_path.canonicalize().ok()?;
    loop {
        let manifest = dir.join(".forklaunch").join("manifest.toml");
        if manifest.exists() {
            let value: toml::Value = toml::from_str(&fs::read_to_string(manifest).ok()?).ok()?;
            return Some(
                value
                    .get("projects")?
                    .as_array()?
                    .iter()
                    .filter_map(|p| {
                        let name = p.get("name")?.as_str()?.to_string();
                        let capabilities = p
                            .get("resources")
                            .and_then(|r| r.get("capabilities"))
                            .and_then(|c| c.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some((name, capabilities))
                    })
                    .collect(),
            );
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Whether the sources use S3 at all (the framework store or the AWS SDK).
fn uses_object_store(sources: &str) -> bool {
    sources.contains("@forklaunch/infrastructure-s3") || sources.contains("@aws-sdk/client-s3")
}

/// Long-lived storage credentials the sources read instead of the task role.
///
/// `S3_ACCESS_KEY_ID` is fine when it only reaches `s3ClientConfig`, which
/// passes keys only when both are set (local MinIO) and otherwise leaves the
/// default credential chain to the task role. Raw AWS keys are never fine.
pub(crate) fn static_storage_credentials(sources: &str) -> Vec<String> {
    let mut found = Vec::new();
    if sources.contains("S3_ACCESS_KEY_ID") && !sources.contains("s3ClientConfig(") {
        found.push("S3_ACCESS_KEY_ID".to_string());
    }
    for key in ["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY"] {
        if sources.contains(key) {
            found.push(key.to_string());
        }
    }
    found
}

/// Code that makes stored files public: public ACLs, a removed public access
/// block, or CORS open to every site.
pub(crate) fn public_storage_access(sources: &str) -> Vec<String> {
    static CORS_ANY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let cors_any = CORS_ANY.get_or_init(|| {
        regex::Regex::new(r#"AllowedOrigins\s*:\s*\[\s*['"]\*['"]"#).expect("cors pattern")
    });
    let mut found: Vec<String> = [
        "public-read",
        "PutPublicAccessBlockCommand",
        "DeletePublicAccessBlockCommand",
    ]
    .iter()
    .filter(|p| sources.contains(**p))
    .map(|p| p.to_string())
    .collect();
    if cors_any.is_match(sources) {
        found.push("AllowedOrigins: ['*']".to_string());
    }
    found
}

/// Bucket administration done from app code. On ForkLaunch the platform owns
/// the bucket, its policy and CORS, and the task role cannot change them.
pub(crate) fn bucket_administration(sources: &str) -> Vec<String> {
    [
        "CreateBucketCommand",
        "PutBucketPolicyCommand",
        "DeleteBucketPolicyCommand",
        "PutBucketCorsCommand",
        "PutBucketAclCommand",
    ]
    .iter()
    .filter(|p| sources.contains(**p))
    .map(|p| p.to_string())
    .collect()
}

/// Browser upload grants with no size limit: a presigned PUT (S3 cannot bound
/// its size) or a presigned POST without a `content-length-range` condition.
pub(crate) fn unbounded_presigned_uploads(sources: &str) -> Vec<String> {
    static PRESIGNED_PUT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let presigned_put = PRESIGNED_PUT.get_or_init(|| {
        regex::Regex::new(r"(?s)getSignedUrl\s*\([^;]{0,300}?PutObjectCommand")
            .expect("presigned put pattern")
    });
    let mut found = Vec::new();
    if presigned_put.is_match(sources) {
        found.push("getSignedUrl(PutObjectCommand)".to_string());
    }
    if sources.contains("createPresignedPost(") && !sources.contains("content-length-range") {
        found.push("createPresignedPost without content-length-range".to_string());
    }
    found
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

            // 6. AI features on health data must not call a provider directly.
            //
            // Microsoft's, Amazon's and Google's BAAs cover only specific
            // services, and a raw provider key sends PHI to whatever the key
            // reaches. The model gateway only offers BAA-covered models to a
            // HIPAA product; anything else needs its own BAA, which this check
            // cannot see, so it asks rather than assumes.
            let has_phi = entities.iter().any(|e| {
                e.field_classifications
                    .values()
                    .any(|classification| classification == "phi")
            });
            if has_phi {
                let sources = read_production_sources(&project_path);
                let providers = direct_ai_providers(&sources);
                if !providers.is_empty() {
                    findings.push(LocalFinding {
                        severity: Severity::Warning,
                        project: project.clone(),
                        check: "ai-provider-direct".to_string(),
                        subject: providers.join(", "),
                        message: format!(
                            "entities hold health data (phi) and this service calls an AI provider directly ({}) — route model calls through the ForkLaunch model gateway (createModelGatewayClient, BAA-covered models only) or confirm the provider has signed a BAA covering this use",
                            providers.join(", ")
                        ),
                    });
                }
            }
        }

        // 7. A managed template must not ship its own provider credentials.
        //
        // Hosted instances never receive them: SMS/email one-time codes go
        // through the platform's instance gateway and model calls through the
        // model gateway, both signed with the instance's own key. A template
        // that reads TWILIO_AUTH_TOKEN or OPENAI_API_KEY either fails on every
        // hosted instance or carries a credential shared by every customer.
        let project_sources = read_production_sources(&project_path);
        if is_managed_instance(&project_path, &project_sources) {
            let mut credentials = provider_credentials_read(&project_sources);
            credentials.retain(|key| !is_email_credential(key));
            if !credentials.is_empty() {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "managed-provider-credentials".to_string(),
                    subject: credentials.join(", "),
                    message: format!(
                        "this service runs as a managed instance but reads provider credentials ({}) — hosted instances do not get them; use the platform's instance gateway for one-time codes and createModelGatewayClient for models",
                        credentials.join(", ")
                    ),
                });
            }

            // 7b. Email goes through the platform, not a provider SDK or key.
            let providers = direct_email_providers(&project_sources);
            if !providers.is_empty() {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "email-provider-direct-in-managed".to_string(),
                    subject: providers.join(", "),
                    message: format!(
                        "this service runs as a managed instance but sends email with its own provider ({}) — hosted instances get no mail credential; send with createEmailClient() (`forklaunch infra add <service> email`), which uses the instance's platform sending identity and suppression list",
                        providers.join(", ")
                    ),
                });
            }
        }

        // 7c. Protected data in an email subject (previews, mail logs).
        let subjects = protected_data_in_email_subjects(&project_sources);
        if !subjects.is_empty() {
            let has_phi = scan_entity_compliance(&project_path)
                .unwrap_or_default()
                .iter()
                .any(|e| e.field_classifications.values().any(|c| c == "phi"));
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "email-protected-data".to_string(),
                subject: subjects.join(", "),
                message: format!(
                    "{}an email subject carries a decrypted protected value ({}) — subjects show in lock-screen and notification previews and in mail logs; keep the protected detail in the body (or behind a link) and use a generic subject",
                    if has_phi { "health data (phi): " } else { "" },
                    subjects.join(", ")
                ),
            });
        }
    }

    // 8. Object storage: provisioned by the platform, reached with the task
    //    role, private, and bounded when a browser uploads to it.
    let declared = declared_object_stores(modules_path);
    for entry in fs::read_dir(modules_path)? {
        let project_path = entry?.path();
        if !project_path.is_dir() {
            continue;
        }
        let project = project_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let sources = read_production_sources(&project_path);
        let registrations =
            fs::read_to_string(project_path.join("registrations.ts")).unwrap_or_default();
        let wired = registrations.contains("S3ObjectStore");
        if let Some(declared_in) = declared.as_ref().and_then(|d| d.get(&project)) {
            let message = match (*declared_in, wired) {
                (true, false) => Some(
                    "the manifest declares an object store but registrations.ts registers no S3ObjectStore — the service has nothing to call it with",
                ),
                (false, true) => Some(
                    "registrations.ts registers an S3ObjectStore but the manifest declares no object store — the platform will not provision a bucket or grant access to one",
                ),
                _ => None,
            };
            if let Some(message) = message {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "object-store-wiring".to_string(),
                    subject: "registrations.ts".to_string(),
                    message: message.to_string(),
                });
            }
        }
        // 9. Platform-held capabilities: the manifest and registrations.ts agree.
        if let Some(declared) = declared_capabilities(modules_path)
            .as_ref()
            .and_then(|d| d.get(&project))
        {
            for capability in crate::infra::capabilities::CAPABILITIES {
                let is_declared = declared.iter().any(|c| c == capability.id);
                let is_wired =
                    registrations.contains(&format!("{}:", capability.registration_key));
                let message = match (is_declared, is_wired) {
                    (true, false) => Some(format!(
                        "the manifest declares {} but registrations.ts registers no {} — the service has nothing to call it with",
                        capability.id, capability.registration_key
                    )),
                    (false, true) => Some(format!(
                        "registrations.ts registers {} but the manifest does not declare {} — the platform will not provision it for this service",
                        capability.registration_key, capability.id
                    )),
                    _ => None,
                };
                if let Some(message) = message {
                    findings.push(LocalFinding {
                        severity: Severity::Warning,
                        project: project.clone(),
                        check: "capability-wiring".to_string(),
                        subject: capability.id.to_string(),
                        message,
                    });
                }
            }
        }

        if !uses_object_store(&sources) {
            continue;
        }
        let credentials = static_storage_credentials(&sources);
        if !credentials.is_empty() {
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "object-store-static-credentials".to_string(),
                subject: credentials.join(", "),
                message: format!(
                    "the S3 client is built from long-lived keys ({}) — deployed on ForkLaunch the service's task role grants access to its own bucket; build the client with s3ClientConfig so keys are used only for local MinIO",
                    credentials.join(", ")
                ),
            });
        }
        let public = public_storage_access(&sources);
        if !public.is_empty() {
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "object-store-public-access".to_string(),
                subject: public.join(", "),
                message: format!(
                    "code makes stored files reachable by anyone ({}) — keep the bucket private and hand out presignDownload links instead",
                    public.join(", ")
                ),
            });
        }
        let administration = bucket_administration(&sources);
        if !administration.is_empty() {
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "object-store-bucket-managed-in-app".to_string(),
                subject: administration.join(", "),
                message: format!(
                    "app code administers the bucket ({}) — the platform creates the bucket and sets its policy and CORS, and the task role is not allowed to change them",
                    administration.join(", ")
                ),
            });
        }
        let unbounded = unbounded_presigned_uploads(&sources);
        if !unbounded.is_empty() {
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "presigned-upload-unbounded".to_string(),
                subject: unbounded.join(", "),
                message: format!(
                    "a browser upload grant has no size limit ({}) — use ObjectStore.presignUpload, a presigned POST that enforces maxBytes and the content type",
                    unbounded.join(", ")
                ),
            });
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

#[cfg(test)]
mod wiring_tests {
    use super::*;

    #[test]
    fn direct_ai_providers_finds_sdk_imports_and_keys_only() {
        let sources = r#"
            import OpenAI from 'openai';
            import { createAnthropic } from "@ai-sdk/anthropic";
            const bedrock = require('@aws-sdk/client-bedrock-runtime');
            const key = process.env.ANTHROPIC_API_KEY;
            import { openai_helpers } from './openai';
            import { createModelGatewayClient } from '@forklaunch/core/http';
        "#;
        assert_eq!(
            direct_ai_providers(sources),
            vec![
                "@ai-sdk/anthropic",
                "@aws-sdk/client-bedrock-runtime",
                "ANTHROPIC_API_KEY",
                "openai"
            ]
        );
        assert!(
            direct_ai_providers("import { createModelGatewayClient } from '@forklaunch/core/http';")
                .is_empty(),
            "the gateway is not a direct provider"
        );
    }

    #[test]
    fn managed_signals_and_credentials() {
        let dir = std::env::temp_dir().join("fl-wiring-managed-signal");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(is_managed_instance(&dir, "getEnvVar('INSTANCE_HMAC_KEY')"));
        assert!(!is_managed_instance(&dir, "getEnvVar('DB_HOST')"));
        std::fs::create_dir_all(dir.join("api/controllers")).unwrap();
        std::fs::write(dir.join("api/controllers/relay.controller.ts"), "").unwrap();
        assert!(is_managed_instance(&dir, ""));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(
            provider_credentials_read("TWILIO_AUTH_TOKEN OPENAI_API_KEY DB_HOST"),
            vec!["OPENAI_API_KEY", "TWILIO_AUTH_TOKEN"]
        );
    }

    fn write_phi_project(proj: &std::path::Path, service_source: &str) {
        std::fs::create_dir_all(proj.join("persistence/entities")).unwrap();
        std::fs::create_dir_all(proj.join("domain/services")).unwrap();
        std::fs::write(
            proj.join("persistence/entities/visit.entity.ts"),
            r#"
            export const VisitEntity = defineComplianceEntity({
              name: 'Visit',
              properties: {
                id: fp.uuid().primary().compliance('none'),
                notes: fp.text().compliance('phi')
              }
            });
            "#,
        )
        .unwrap();
        std::fs::write(
            proj.join("registrations.ts"),
            "registerEncryptor(x); ComplianceDataService; wrapEmWithTenantContext(em)",
        )
        .unwrap();
        std::fs::write(proj.join("mikro-orm.config.ts"), "export default {};").unwrap();
        std::fs::write(proj.join("domain/services/summary.service.ts"), service_source).unwrap();
    }

    #[test]
    fn a_phi_service_calling_a_provider_directly_is_flagged() {
        let dir = std::env::temp_dir().join("fl-wiring-ai-direct");
        let _ = std::fs::remove_dir_all(&dir);
        write_phi_project(
            &dir.join("clinic"),
            "import OpenAI from 'openai'; const c = new OpenAI({ apiKey: getEnvVar('OPENAI_API_KEY') });",
        );
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let f = findings
            .iter()
            .find(|f| f.check == "ai-provider-direct")
            .unwrap_or_else(|| panic!("expected ai-provider-direct, got {:?}", findings));
        assert!(matches!(f.severity, Severity::Warning));
        assert_eq!(f.subject, "OPENAI_API_KEY, openai");
        assert!(f.message.contains("createModelGatewayClient"));
    }

    #[test]
    fn a_phi_service_using_the_gateway_is_not_flagged() {
        let dir = std::env::temp_dir().join("fl-wiring-ai-gateway");
        let _ = std::fs::remove_dir_all(&dir);
        write_phi_project(
            &dir.join("clinic"),
            "import { createModelGatewayClient } from '@forklaunch/core/http';",
        );
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            !findings.iter().any(|f| f.check == "ai-provider-direct"),
            "{:?}",
            findings
        );
    }

    #[test]
    fn a_non_phi_service_may_call_a_provider() {
        let dir = std::env::temp_dir().join("fl-wiring-ai-no-phi");
        let proj = dir.join("svc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(proj.join("persistence/entities")).unwrap();
        std::fs::write(
            proj.join("persistence/entities/note.entity.ts"),
            "export const N = defineComplianceEntity({ name: 'N', properties: { body: fp.text().compliance('pii') } });",
        )
        .unwrap();
        std::fs::write(proj.join("registrations.ts"), "import OpenAI from 'openai';").unwrap();
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!findings.iter().any(|f| f.check == "ai-provider-direct"));
    }

    #[test]
    fn a_managed_template_reading_provider_credentials_is_flagged() {
        let dir = std::env::temp_dir().join("fl-wiring-managed-creds");
        let proj = dir.join("iam");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("registrations.ts"),
            "const gateway = getEnvVar('PLATFORM_GATEWAY_URL'); const t = getEnvVar('TWILIO_AUTH_TOKEN');",
        )
        .unwrap();
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let f = findings
            .iter()
            .find(|f| f.check == "managed-provider-credentials")
            .unwrap_or_else(|| panic!("expected the finding, got {:?}", findings));
        assert_eq!(f.subject, "TWILIO_AUTH_TOKEN");
    }

    #[test]
    fn an_ordinary_app_reading_twilio_is_not_a_managed_finding() {
        let dir = std::env::temp_dir().join("fl-wiring-unmanaged-creds");
        let proj = dir.join("messaging");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("registrations.ts"), "getEnvVar('TWILIO_AUTH_TOKEN')").unwrap();
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!findings.iter().any(|f| f.check == "managed-provider-credentials"));
    }
}

#[cfg(test)]
mod object_store_tests {
    use super::*;

    fn app(name: &str, manifest_object_store: bool, registrations: &str, extra: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("fl-object-store-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let modules = root.join("src/modules");
        let service = modules.join("files");
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        std::fs::create_dir_all(&service).unwrap();
        let resources = if manifest_object_store {
            "[projects.resources]\nobject_store = \"s3\"\n"
        } else {
            "[projects.resources]\ndatabase = \"postgresql\"\n"
        };
        std::fs::write(
            root.join(".forklaunch/manifest.toml"),
            format!("app_name = \"demo\"\n\n[[projects]]\nname = \"files\"\n{resources}"),
        )
        .unwrap();
        std::fs::write(service.join("registrations.ts"), registrations).unwrap();
        std::fs::write(service.join("uploads.ts"), extra).unwrap();
        modules
    }

    const KEYLESS: &str = "import { S3ObjectStore, s3ClientConfig } from '@forklaunch/infrastructure-s3';\n\
        new S3ObjectStore(otel, { bucket: S3_BUCKET, clientConfig: s3ClientConfig({ url: S3_URL, region: S3_REGION, accessKeyId: S3_ACCESS_KEY_ID, secretAccessKey: S3_SECRET_ACCESS_KEY }) });";

    fn checks(modules: &std::path::Path) -> Vec<String> {
        run_local_checks(modules)
            .unwrap()
            .into_iter()
            .filter(|f| f.check.starts_with("object-store") || f.check == "presigned-upload-unbounded")
            .map(|f| f.check)
            .collect()
    }

    #[test]
    fn keyless_declared_and_wired_store_is_clean() {
        let modules = app("clean", true, KEYLESS, "await store.presignUpload(key, { contentType, maxBytes: 1000 });");
        assert!(checks(&modules).is_empty(), "{:?}", checks(&modules));
    }

    #[test]
    fn declared_and_wired_must_agree() {
        let modules = app("undeclared", false, KEYLESS, "");
        assert_eq!(checks(&modules), vec!["object-store-wiring"]);
        let modules = app("unwired", true, "export {}", "");
        assert_eq!(checks(&modules), vec!["object-store-wiring"]);
    }

    #[test]
    fn static_keys_are_flagged_unless_behind_s3_client_config() {
        let old = "import { S3ObjectStore } from '@forklaunch/infrastructure-s3';\n\
            new S3ObjectStore(otel, { bucket: S3_BUCKET, clientConfig: { credentials: { accessKeyId: S3_ACCESS_KEY_ID, secretAccessKey: S3_SECRET_ACCESS_KEY } } });";
        let modules = app("static", true, old, "");
        assert_eq!(checks(&modules), vec!["object-store-static-credentials"]);
        assert_eq!(
            static_storage_credentials("import '@aws-sdk/client-s3'; process.env.AWS_SECRET_ACCESS_KEY"),
            vec!["AWS_SECRET_ACCESS_KEY"]
        );
    }

    #[test]
    fn public_access_bucket_admin_and_unbounded_uploads() {
        let code = "import { PutBucketCorsCommand, PutObjectCommand } from '@aws-sdk/client-s3';\n\
            await s3.send(new PutObjectCommand({ Bucket, Key, ACL: 'public-read' }));\n\
            await s3.send(new PutBucketCorsCommand({ CORSConfiguration: { CORSRules: [{ AllowedOrigins: ['*'] }] } }));\n\
            const url = await getSignedUrl(s3, new PutObjectCommand({ Bucket, Key }), { expiresIn: 3600 });";
        let modules = app("risky", true, KEYLESS, code);
        let mut found = checks(&modules);
        found.sort();
        assert_eq!(
            found,
            vec![
                "object-store-bucket-managed-in-app",
                "object-store-public-access",
                "presigned-upload-unbounded"
            ]
        );
        assert_eq!(
            public_storage_access(code),
            vec!["public-read", "AllowedOrigins: ['*']"]
        );
        assert_eq!(
            unbounded_presigned_uploads("createPresignedPost(s3, { Bucket, Key })"),
            vec!["createPresignedPost without content-length-range"]
        );
        assert!(unbounded_presigned_uploads(
            "createPresignedPost(s3, { Conditions: [['content-length-range', 1, 10]] })"
        )
        .is_empty());
    }

    #[test]
    fn object_store_checks_score_through_the_report_card() {
        for id in [
            "object-store-wiring",
            "object-store-static-credentials",
            "object-store-public-access",
            "object-store-bucket-managed-in-app",
            "presigned-upload-unbounded",
        ] {
            assert!(LOCAL_CHECK_IDS.contains(&id), "{id} missing from LOCAL_CHECK_IDS");
        }
    }
}

#[cfg(test)]
mod email_tests {
    use super::*;

    fn project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fl-email-checks-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in files {
            let path = dir.join("svc").join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    fn found(dir: &std::path::Path) -> Vec<(String, String)> {
        let findings = run_local_checks(dir).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        findings
            .into_iter()
            .filter(|f| {
                f.check.starts_with("email-") || f.check == "managed-provider-credentials"
            })
            .map(|f| (f.check, f.subject))
            .collect()
    }

    const MANAGED: &str = "getEnvVar('INSTANCE_HMAC_KEY');\n";

    #[test]
    fn a_managed_service_with_its_own_mail_provider_is_flagged_once() {
        let dir = project(
            "direct",
            &[(
                "registrations.ts",
                &format!(
                    "{MANAGED}import nodemailer from 'nodemailer';\nimport {{ SESv2Client }} from '@aws-sdk/client-sesv2';\nconst key = getEnvVar('SENDGRID_API_KEY'); const host = getEnvVar('SMTP_HOST');"
                ),
            )],
        );
        assert_eq!(
            found(&dir),
            vec![(
                "email-provider-direct-in-managed".to_string(),
                "@aws-sdk/client-sesv2, SENDGRID_API_KEY, SMTP_HOST, nodemailer".to_string()
            )],
            "SENDGRID_API_KEY is reported by the email check, not twice"
        );
    }

    #[test]
    fn email_credentials_leave_other_provider_credentials_to_the_generic_check() {
        let dir = project(
            "mixed",
            &[(
                "registrations.ts",
                &format!("{MANAGED}getEnvVar('TWILIO_AUTH_TOKEN'); getEnvVar('POSTMARK_SERVER_TOKEN');"),
            )],
        );
        let mut f = found(&dir);
        f.sort();
        assert_eq!(
            f,
            vec![
                ("email-provider-direct-in-managed".to_string(), "POSTMARK_SERVER_TOKEN".to_string()),
                ("managed-provider-credentials".to_string(), "TWILIO_AUTH_TOKEN".to_string()),
            ]
        );
    }

    #[test]
    fn an_unmanaged_app_or_the_platform_client_is_not_flagged() {
        let dir = project(
            "unmanaged",
            &[("registrations.ts", "import nodemailer from 'nodemailer'; getEnvVar('SMTP_HOST');")],
        );
        assert!(found(&dir).is_empty());
        let dir = project(
            "gateway",
            &[(
                "registrations.ts",
                &format!("{MANAGED}import {{ createEmailClient }} from '@forklaunch/core/http'; const smtpish = 'SMTP';"),
            )],
        );
        assert!(found(&dir).is_empty());
    }

    #[test]
    fn deanon_in_an_email_subject_is_flagged_and_critical_with_phi() {
        let code = "await this.email.send({\n  to: patient.email.deanon,\n  subject: `Results for ${patient.name.deanon}, ready`,\n  text: `Hi ${patient.name.deanon}`\n});";
        let dir = project(
            "subject-phi",
            &[
                ("services/notify.ts", code),
                (
                    "persistence/entities/patient.entity.ts",
                    "export const P = defineComplianceEntity({ name: 'P', properties: { diagnosis: fp.text().compliance('phi') } });",
                ),
            ],
        );
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let f = findings
            .iter()
            .find(|f| f.check == "email-protected-data")
            .unwrap_or_else(|| panic!("{findings:?}"));
        assert_eq!(f.subject, "`Results for ${patient.name.deanon}, ready`");
        assert!(f.message.starts_with("health data (phi)"));
    }

    #[test]
    fn subject_detection_is_conservative() {
        // Recipient and body may hold the recipient's own data.
        assert!(protected_data_in_email_subjects(
            "EmailClient.send({ to: user.email.deanon, subject: 'Welcome', html: body.deanon })"
        )
        .is_empty());
        // Not an email send.
        assert!(protected_data_in_email_subjects(
            "res.status(200).send({ subject: row.subject.deanon }); queue.send({ subject: x.deanon })"
        )
        .is_empty());
        // nodemailer and SES shapes.
        assert_eq!(
            protected_data_in_email_subjects("transporter.sendMail({ from, to, subject: p.ssn.deanon })"),
            vec!["p.ssn.deanon"]
        );
        assert_eq!(
            protected_data_in_email_subjects(
                "new SendEmailCommand({ Content: { Simple: { Subject: { Data: p.name.deanon }, Body } } })"
            ),
            vec!["{ Data: p.name.deanon }"]
        );
    }

    #[test]
    fn email_checks_score_through_the_report_card() {
        for id in ["email-provider-direct-in-managed", "email-protected-data"] {
            assert!(LOCAL_CHECK_IDS.contains(&id), "{id} missing from LOCAL_CHECK_IDS");
        }
    }
}
