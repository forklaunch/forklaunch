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
use crate::core::static_analysis::route_analyzer;

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
    "route-auth-missing",
    "route-outside-framework",
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
    "sms-provider-direct-in-managed",
    "sms-protected-data",
    "whatsapp-provider-direct-in-managed",
    "whatsapp-protected-data",
    "voice-provider-direct-in-managed",
    "voice-protected-data",
    "payments-stripe-keys-in-managed",
    "stripe-webhook-unverified",
    "payments-protected-data",
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
    "VONAGE_API_KEY",
    "VONAGE_API_SECRET",
    "NEXMO_API_KEY",
    "NEXMO_API_SECRET",
    "MESSAGEBIRD_ACCESS_KEY",
    "MESSAGEBIRD_API_KEY",
];

/// SMS vendor SDKs a managed service must not call: the platform sends texts
/// through its End User Messaging gateway (createSmsClient). Their keys are
/// reported by `managed-provider-credentials`, so this only names SDKs.
const SMS_PROVIDER_PACKAGES: &[&str] = &["twilio", "vonage", "messagebird"];
const SMS_PROVIDER_PREFIXES: &[&str] = &["@aws-sdk/client-pinpoint", "@vonage/"];
/// SNS is a general pub/sub SDK; it sends SMS only when publishing to a
/// phone number.
const SNS_PACKAGE: &str = "@aws-sdk/client-sns";

/// SMS vendor SDKs the sources import. Sorted and de-duplicated.
pub(crate) fn direct_sms_providers(sources: &str) -> Vec<String> {
    let sns_texts = sources.contains("PhoneNumber") || sources.contains("SetSMSAttributes");
    let mut found: Vec<String> = imported_modules(sources)
        .into_iter()
        .filter(|module| {
            SMS_PROVIDER_PACKAGES.contains(&module.as_str())
                || SMS_PROVIDER_PREFIXES.iter().any(|p| module.starts_with(p))
                || (module == SNS_PACKAGE && sns_texts)
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The text between the parenthesis at `open` and its match.
fn balanced_args(text: &str, open: usize) -> &str {
    let mut depth = 0usize;
    for (i, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    &text[open + 1..]
}

/// Calls in one file that hand a `.deanon` value (plaintext of a compliant
/// field) to an SMS send: the framework client (`sms.send`,
/// `this.smsClient.send`), Twilio's `messages.create`, End User Messaging's
/// `SendTextMessageCommand` and an SNS `PublishCommand` to a phone number.
/// A variable assigned from `.deanon` (directly or through other variables)
/// counts too. Returns a short description per call.
pub(crate) fn sms_sends_with_protected_data(source: &str) -> Vec<String> {
    static SENDS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static ASSIGN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let sends = SENDS.get_or_init(|| {
        regex::Regex::new(
            r"(?:\b[\w$]*[sS][mM][sS][\w$]*\s*\??\.\s*send|\.messages\s*\.\s*create|\bnew\s+SendTextMessageCommand|\bnew\s+PublishCommand)\s*\(",
        )
        .expect("sms send pattern")
    });
    let assign = ASSIGN.get_or_init(|| {
        regex::Regex::new(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;]+)?=\s*([^;]*)")
            .expect("assignment pattern")
    });
    if !source.contains(".deanon") {
        return Vec::new();
    }
    // Variables that hold plaintext, propagated through a few assignments.
    let assignments: Vec<(String, String)> = assign
        .captures_iter(source)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let mut tainted: Vec<String> = Vec::new();
    for _ in 0..3 {
        for (name, value) in &assignments {
            if tainted.contains(name) {
                continue;
            }
            let from_tainted = tainted.iter().any(|t| mentions(value, t));
            if value.contains(".deanon") || from_tainted {
                tainted.push(name.clone());
            }
        }
    }
    let twilio = source.contains("'twilio'") || source.contains("\"twilio\"");
    let mut found = Vec::new();
    for m in sends.find_iter(source) {
        let call = m.as_str();
        if call.contains("messages") && !twilio {
            continue;
        }
        let args = balanced_args(source, m.end() - 1);
        if call.contains("PublishCommand") && !args.contains("PhoneNumber") {
            continue;
        }
        // Only the text of the message counts: the destination number is
        // rightly a `.deanon` value.
        let leaks = message_bodies(args)
            .iter()
            .any(|body| body.contains(".deanon") || tainted.iter().any(|t| mentions(body, t)));
        if leaks {
            let line = source[..m.start()].matches('\n').count() + 1;
            let callee: String = call.trim_end_matches('(').split_whitespace().collect();
            found.push(format!("{callee} (line {line})"));
        }
    }
    found
}

/// The expressions given as a message's text in a send's arguments:
/// `body:`/`Body:` (framework, Twilio), `MessageBody:` (End User Messaging),
/// `Message:` (SNS), or the shorthand `{ body }`.
fn message_bodies(args: &str) -> Vec<String> {
    static KEY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let key = KEY.get_or_init(|| {
        regex::Regex::new(r"(?:^|[{,\s])(?:body|Body|MessageBody|Message)\s*(:|[,}])")
            .expect("body key pattern")
    });
    let mut out = Vec::new();
    for c in key.captures_iter(args) {
        let marker = c.get(1).unwrap();
        if marker.as_str() != ":" {
            out.push("body".to_string());
            continue;
        }
        // The value runs to the next top-level comma or the object's end.
        let rest = &args[marker.end()..];
        let mut depth = 0i32;
        let mut end = rest.len();
        let mut in_template = false;
        for (i, ch) in rest.char_indices() {
            match ch {
                '`' => in_template = !in_template,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth > 0 => depth -= 1,
                '}' | ')' if depth == 0 && !in_template => {
                    end = i;
                    break;
                }
                ',' if depth == 0 && !in_template => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        out.push(rest[..end].trim().to_string());
    }
    out
}

/// Whether `text` uses the identifier `name` (not as part of a longer one).
fn mentions(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(name).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + name.len()..].chars().next();
        !before.is_some_and(|c| is_ident(c) || c == '.') && !after.is_some_and(is_ident)
    })
}

/// Production TypeScript files, relative path and text, for per-file checks.
fn production_files(project_path: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !NON_PRODUCTION_DIRS.contains(&name.as_str()) {
                    walk(root, &path, out);
                }
            } else if name.ends_with(".ts") && !name.ends_with(".test.ts") {
                if let Ok(text) = fs::read_to_string(&path) {
                    let rel = path.strip_prefix(root).unwrap_or(&path);
                    out.push((rel.to_string_lossy().to_string(), text));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(project_path, project_path, &mut out);
    out.sort();
    out
}

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

/// Stripe credentials a service might read.
const STRIPE_KEYS: &[&str] = &["STRIPE_API_KEY", "STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET"];

/// Stripe credentials a service depends on: a key declared as a REQUIRED
/// config value or read straight from `process.env`, and a Stripe client built
/// with `new Stripe(` where `createStripeClient` is never used.
///
/// A key declared `optional(...)` and only used outside managed mode
/// (`createStripeClient({ Stripe, apiKey })`, or an `isManagedInstance()`
/// branch) is fine: a hosted instance runs without it.
pub(crate) fn managed_stripe_credentials(sources: &str) -> Vec<String> {
    let mut found = Vec::new();
    for key in STRIPE_KEYS {
        let entry = regex::Regex::new(&format!(
            r"\b{key}\s*:\s*\{{[^{{}}]*?\btype\s*:\s*([^,\n}}]+)"
        ))
        .expect("stripe key entry pattern");
        let required = entry
            .captures_iter(sources)
            .any(|c| !c[1].trim().starts_with("optional("));
        let direct = regex::Regex::new(&format!(
            r#"process\.env(?:\.{key}\b|\[\s*['"]{key}['"]\s*\])"#
        ))
        .expect("process.env pattern")
        .is_match(sources);
        if required || direct {
            found.push(key.to_string());
        }
    }
    if sources.contains("new Stripe(") && !sources.contains("createStripeClient") {
        found.push("new Stripe(".to_string());
    }
    found
}

/// Signs that a service acts on Stripe webhook events.
const STRIPE_WEBHOOK_MARKERS: &[&str] = &[
    "stripe-signature",
    "Stripe.Event",
    "checkout.session.completed",
    "invoice.payment_succeeded",
    "customer.subscription.",
];

/// Whether the sources act on Stripe webhook events without verifying them:
/// neither Stripe's signature (`constructEvent`) nor the platform's
/// (`verifyPlatformEvent`, for events the platform relays) is checked.
pub(crate) fn stripe_webhook_unverified(sources: &str) -> bool {
    STRIPE_WEBHOOK_MARKERS.iter().any(|m| sources.contains(m))
        && !["constructEvent(", "constructEventAsync(", "verifyPlatformEvent("]
            .iter()
            .any(|v| sources.contains(v))
}

/// Index of the bracket that closes a group opened just before `from`.
fn closing_bracket(text: &str, from: usize) -> usize {
    let mut depth = 1i32;
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for (i, c) in text[from..].char_indices() {
        if let Some(q) = quote {
            if c == q && prev != '\\' {
                quote = None;
            }
        } else {
            match c {
                '\'' | '"' | '`' => quote = Some(c),
                '(' | '{' | '[' => depth += 1,
                ')' | '}' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return from + i;
                    }
                }
                _ => {}
            }
        }
        prev = c;
    }
    text.len()
}

/// The text of a property value starting at the beginning of `rest`: a
/// bracketed group, or everything up to the next comma or newline at its depth.
fn stripe_property_value(rest: &str) -> &str {
    if rest.starts_with(['{', '[', '(']) {
        let end = closing_bracket(rest, 1);
        return &rest[..(end + 1).min(rest.len())];
    }
    let mut depth = 0i32;
    let end = rest
        .char_indices()
        .find(|(_, ch)| match ch {
            '(' | '{' | '[' => {
                depth += 1;
                false
            }
            ')' | '}' | ']' => {
                depth -= 1;
                depth < 0
            }
            ',' | '\n' => depth == 0,
            _ => false,
        })
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Stripe calls that send a `.deanon` value (compliant fields expose plaintext
/// only through `.deanon`) as `metadata`, `description` or
/// `statement_descriptor`: protected data leaving for Stripe, which signs no
/// BAA. Heuristic: the value must be written inside the call expression, so a
/// plaintext assigned to a variable first is not seen.
pub(crate) fn stripe_calls_with_protected_data(sources: &str) -> Vec<String> {
    static CALL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static FIELD: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let call = CALL.get_or_init(|| {
        regex::Regex::new(
            r"\b[sS]tripe\w*\s*\.\s*([A-Za-z]+(?:\s*\.\s*[A-Za-z]+)?)\s*\.\s*(create|update)\s*\(",
        )
        .expect("stripe call pattern")
    });
    let field = FIELD.get_or_init(|| {
        regex::Regex::new(r"\b(metadata|description|statement_descriptor(?:_suffix)?)\s*:\s*")
            .expect("stripe field pattern")
    });
    let mut found = Vec::new();
    for c in call.captures_iter(sources) {
        let open = c.get(0).unwrap().end();
        let args = &sources[open..closing_bracket(sources, open)];
        let leaks = field.captures_iter(args).any(|f| {
            stripe_property_value(&args[f.get(0).unwrap().end()..]).contains(".deanon")
        });
        if leaks {
            let name = format!("{}.{}", c[1].split_whitespace().collect::<String>(), &c[2]);
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found
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

/// Ways a service reaches WhatsApp without the platform: Meta's Cloud API
/// host, WhatsApp/Meta token variables, and WhatsApp SDKs (Meta's, community
/// Cloud API wrappers, and AWS End User Messaging Social itself). Sorted.
pub(crate) fn direct_whatsapp_access(sources: &str) -> Vec<String> {
    static TOKENS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let tokens = TOKENS.get_or_init(|| {
        regex::Regex::new(
            r"\b(?:WHATSAPP_[A-Z0-9_]*(?:TOKEN|SECRET|API_KEY)|META_[A-Z0-9_]*(?:TOKEN|SECRET|APP_KEY|API_KEY))\b",
        )
        .expect("whatsapp token pattern")
    });
    let mut found: Vec<String> = tokens
        .find_iter(sources)
        .map(|m| m.as_str().to_string())
        .collect();
    if sources.contains("graph.facebook.com") {
        found.push("graph.facebook.com".to_string());
    }
    found.extend(imported_modules(sources).into_iter().filter(|m| {
        matches!(
            m.as_str(),
            "whatsapp" | "whatsapp-cloud-api" | "whatsapp-api-js" | "@aws-sdk/client-socialmessaging"
        )
    }));
    found.sort();
    found.dedup();
    found
}

/// WhatsApp sends in one source file whose arguments carry a compliant
/// field's plaintext (`.deanon`), directly or through a variable assigned
/// from it. Returns `line: call` for each.
///
/// Only files that mention WhatsApp are read, and only calls on a
/// WhatsApp-named receiver (`whatsapp.sendText(`, `tokens.WhatsAppClient).sendTemplate(`)
/// or `SendWhatsAppMessageCommand(` count, so SMS or email sends with the
/// same method names are not flagged here.
pub(crate) fn whatsapp_protected_sends(source: &str) -> Vec<String> {
    static SEND: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static ASSIGN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let send = SEND.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(?:whats_?app\w*\)?\s*\.\s*(?:sendTemplate|sendText)|SendWhatsAppMessageCommand)\s*\(",
        )
        .expect("whatsapp send pattern")
    });
    let assign = ASSIGN.get_or_init(|| {
        regex::Regex::new(r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;]+)?=([^;]*)")
            .expect("assignment pattern")
    });
    if !source.to_ascii_lowercase().contains("whatsapp") {
        return Vec::new();
    }
    // Variables holding plaintext, followed through a few assignments.
    let mut tainted: Vec<String> = Vec::new();
    for _ in 0..3 {
        for caps in assign.captures_iter(source) {
            let name = caps[1].to_string();
            if tainted.contains(&name) {
                continue;
            }
            let rhs = &caps[2];
            if rhs.contains(".deanon") || tainted.iter().any(|t| mentions(rhs, t)) {
                tainted.push(name);
            }
        }
    }
    let mut found = Vec::new();
    for m in send.find_iter(source) {
        let args = call_args(&source[m.end()..]);
        let leaks = args.contains(".deanon") || tainted.iter().any(|t| mentions(args, t));
        if leaks {
            let line = source[..m.start()].matches('\n').count() + 1;
            let call: String = m.as_str().split_whitespace().collect();
            found.push(format!("{line}: {}", call.trim_end_matches('(')));
        }
    }
    found
}

/// The argument text of a call, from just after `(` to its matching `)`
/// (bounded, so a broken file cannot make this scan everything).
fn call_args(rest: &str) -> &str {
    let mut depth = 1usize;
    for (i, c) in rest.char_indices() {
        if i > 4000 {
            return &rest[..i];
        }
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..i];
                }
            }
            _ => {}
        }
    }
    rest
}

/// Marks a `whatsapp-protected-data` finding on a service whose entities hold
/// phi; the report card scores those critical.
pub(crate) const WHATSAPP_PHI_MARKER: &str = "entities hold health data (phi)";

/// Run all local checks for every project under `modules_path`.
pub(crate) fn run_local_checks(modules_path: &Path) -> Result<Vec<LocalFinding>> {
    let mut findings: Vec<LocalFinding> = Vec::new();

    if !modules_path.exists() {
        return Ok(findings);
    }

    // Whether any service holds health data: a `.deanon` value texted from
    // anywhere in the app may be it.
    let app_has_phi = fs::read_dir(modules_path)?.flatten().any(|entry| {
        entry.path().is_dir()
            && scan_entity_compliance(&entry.path())
                .unwrap_or_default()
                .iter()
                .any(|e| e.field_classifications.values().any(|c| c == "phi"))
    });

    for entry in fs::read_dir(modules_path)? {
        let entry = entry?;
        let project_path = entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let project = entry.file_name().to_string_lossy().to_string();

        let owns_persistence = project_path.join("persistence").is_dir();

        // 0. Route protection, read from each route's contract. A non-public contract with no
        //    auth method lets callers in without a credential; a route registered on the raw
        //    application with an inline handler gets none of the framework's auth or validation.
        for route in route_analyzer::scan_routes(&project_path) {
            let at = format!("{} {} ({}:{})", route.method, route.path, route.file, route.line);
            if route.missing_auth() {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "route-auth-missing".to_string(),
                    subject: at,
                    message: format!(
                        "Declares access '{}' but no auth method, so the framework lets callers in without a credential. Add an auth block (e.g. auth: jwtAuth(ROLES)) or mark the route access: 'public' if it is meant to be open.",
                        route.access.as_deref().unwrap_or("(none)")
                    ),
                });
            } else if route.source == route_analyzer::RouteSource::OutsideFramework {
                findings.push(LocalFinding {
                    severity: Severity::Info,
                    project: project.clone(),
                    check: "route-outside-framework".to_string(),
                    subject: at,
                    message: "Registered on the raw application with an inline handler, so ForkLaunch's access levels, auth and validation do not apply. Confirm it is meant to be open, or declare it with handlers.* and a contract.".to_string(),
                });
            }
        }

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

            // Payments in a managed instance go through the platform's Stripe
            // Connect gateway: no Stripe key in the app.
            let stripe = managed_stripe_credentials(&project_sources);
            if !stripe.is_empty() {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "payments-stripe-keys-in-managed".to_string(),
                    subject: stripe.join(", "),
                    message: format!(
                        "this service runs as a managed instance but depends on Stripe credentials ({}) — a hosted instance has none; build the client with createStripeClient() (forklaunch infra add <service> payments) and declare any key optional for use outside managed mode",
                        stripe.join(", ")
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
            // SMS vendor SDKs (their keys are the finding above).
            let sms_sdks = direct_sms_providers(&project_sources);
            if !sms_sdks.is_empty() {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "sms-provider-direct-in-managed".to_string(),
                    subject: sms_sdks.join(", "),
                    message: format!(
                        "this service runs as a managed instance but calls an SMS vendor SDK directly ({}) — hosted instances hold no vendor credential or number; send with createSmsClient() (`forklaunch infra add <service> sms`), which the platform limits, attributes and opts out per instance",
                        sms_sdks.join(", ")
                    ),
                });
            }
        }

        // 7b. Protected data must not be texted: SMS is not a secure channel.
        for (file, text) in production_files(&project_path) {
            let calls = sms_sends_with_protected_data(&text);
            if calls.is_empty() {
                continue;
            }
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "sms-protected-data".to_string(),
                subject: format!("{file}: {}", calls.join(", ")),
                message: format!(
                    "{}a `.deanon` value (the plaintext of a compliant field) reaches an SMS body in {file} — texts are not a secure channel: carriers store them and lock screens show them. Send a sign-in link or a neutral notice instead",
                    if app_has_phi {
                        "health data (phi): "
                    } else {
                        ""
                    }
                ),
            });
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

        // 7b. WhatsApp goes through the platform, and never carries protected
        //     data: WhatsApp is not covered by the AWS BAA and Meta signs none.
        if is_managed_instance(&project_path, &project_sources) {
            let direct = direct_whatsapp_access(&project_sources);
            if !direct.is_empty() {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "whatsapp-provider-direct-in-managed".to_string(),
                    subject: direct.join(", "),
                    message: format!(
                        "this service runs as a managed instance but reaches WhatsApp directly ({}) — hosted instances hold no Meta token or AWS credential; send with createWhatsAppClient() (forklaunch infra add <service> whatsapp)",
                        direct.join(", ")
                    ),
                });
            }
        }
        let holds_phi = entities
            .iter()
            .any(|e| e.field_classifications.values().any(|c| c == "phi"));
        for (file, text) in production_files(&project_path) {
            for send in whatsapp_protected_sends(&text) {
                findings.push(LocalFinding {
                    severity: Severity::Warning,
                    project: project.clone(),
                    check: "whatsapp-protected-data".to_string(),
                    subject: format!("{file}:{send}"),
                    message: format!(
                        "{}a compliant field's plaintext (.deanon) is sent over WhatsApp ({file}:{send}) — WhatsApp is not covered by the AWS BAA and Meta signs no BAA; send a template that carries no protected data (a reminder to open the app) instead",
                        if holds_phi { format!("{WHATSAPP_PHI_MARKER} and ") } else { String::new() }
                    ),
                });
            }
        }

        // 7b. Voice: a managed service calls through the platform's Connect
        //     instance (createVoiceClient), never a voice vendor of its own;
        //     and no protected value rides along in call attributes, which
        //     Connect keeps in its contact records.
        let has_phi_entities = entities.iter().any(|e| {
            e.field_classifications
                .values()
                .any(|classification| classification == "phi")
        });
        findings.extend(voice_findings(
            &project,
            is_managed_instance(&project_path, &project_sources),
            has_phi_entities,
            &project_sources,
        ));

        // 10. Stripe webhooks are acted on only after a signature check. Only
        //     a service's own endpoints (api/) receive them: a library that
        //     processes events its caller verified is not the receiver.
        let receives_stripe_events = STRIPE_WEBHOOK_MARKERS
            .iter()
            .any(|m| read_production_sources(&project_path.join("api")).contains(m));
        if receives_stripe_events && stripe_webhook_unverified(&project_sources) {
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "stripe-webhook-unverified".to_string(),
                subject: "webhook".to_string(),
                message: "this service handles Stripe webhook events but never verifies a signature — check Stripe's with stripe.webhooks.constructEvent(rawBody, signature, secret), or, in managed mode, receive them as platform events verified with verifyPlatformEvent".to_string(),
            });
        }

        // 11. Protected data sent to Stripe (no BAA) as metadata or text.
        let leaks = stripe_calls_with_protected_data(&project_sources);
        if !leaks.is_empty() {
            let phi = entities
                .iter()
                .any(|e| e.field_classifications.values().any(|c| c == "phi"));
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.clone(),
                check: "payments-protected-data".to_string(),
                subject: format!("{}{}", if phi { "phi: " } else { "" }, leaks.join(", ")),
                message: format!(
                    "a decrypted (.deanon) value is sent to Stripe as metadata or a description ({}) — Stripe signs no BAA, and these fields are shown to anyone with dashboard access; send an opaque reference (the record id) instead",
                    leaks.join(", ")
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
                // A registration only counts as the capability in a service that
                // reads the gateway contract: billing-stripe's own keyed
                // `StripeClient` is ordinary Stripe, not undeclared payments.
                let is_wired = registrations
                    .contains(&format!("{}:", capability.registration_key))
                    && (is_declared || is_managed_instance(&project_path, &sources));
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

// ------------------------------------------------------------------ voice

/// Voice vendor SDKs a managed service must not call itself. `twilio` counts
/// only where the code uses its voice API: the same SDK sends SMS, and its
/// credentials are `managed-provider-credentials`' to report.
const VOICE_PROVIDER_PACKAGES: &[&str] = &[
    "@aws-sdk/client-connect",
    "@aws-sdk/client-connectcontactlens",
    "@vonage/voice",
    "@vonage/server-sdk",
    "@vonage/vcr-sdk",
];

/// Signs that code uses Twilio's voice API rather than its messaging API.
const TWILIO_VOICE_MARKERS: &[&str] = &[".calls.create(", "VoiceResponse", "twiml.voice"];

/// Voice credentials and ids the platform holds (Twilio's are reported by
/// `managed-provider-credentials`, so they are not repeated here).
const VOICE_PROVIDER_KEYS: &[&str] = &[
    "CONNECT_INSTANCE_ID",
    "AMAZON_CONNECT_INSTANCE_ID",
    "CONNECT_CONTACT_FLOW_ID",
    "VONAGE_API_KEY",
    "VONAGE_API_SECRET",
    "VONAGE_APPLICATION_ID",
    "VONAGE_PRIVATE_KEY",
];

/// Voice vendors the sources reach directly: SDK imports and credential reads.
pub(crate) fn direct_voice_providers(sources: &str) -> Vec<String> {
    let modules = imported_modules(sources);
    let mut found: Vec<String> = modules
        .iter()
        .filter(|m| VOICE_PROVIDER_PACKAGES.contains(&m.as_str()))
        .cloned()
        .collect();
    if modules.iter().any(|m| m == "twilio")
        && TWILIO_VOICE_MARKERS.iter().any(|marker| sources.contains(marker))
    {
        found.push("twilio (voice)".to_string());
    }
    found.extend(
        VOICE_PROVIDER_KEYS
            .iter()
            .filter(|key| contains_word(sources, key))
            .map(|key| key.to_string()),
    );
    found.sort();
    found.dedup();
    found
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The text between the bracket at `open` and its match, skipping strings
/// and comments well enough for call arguments.
fn balanced(text: &str, open: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else {
            match c {
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return text.get(open + 1..i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Whether `expr` exposes a compliant field's plaintext (`x.deanon`).
fn reads_deanon(expr: &str) -> bool {
    expr.match_indices(".deanon")
        .any(|(i, _)| !expr[i + 7..].chars().next().is_some_and(is_ident_char))
}

/// Identifiers declared from an expression that reads `.deanon`
/// (`const diagnosis = record.diagnosis.deanon;`), one level deep.
fn deanon_identifiers(sources: &str) -> Vec<String> {
    static DECL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = DECL.get_or_init(|| {
        regex::Regex::new(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;]+)?=([^;\n]*)")
            .expect("declaration pattern")
    });
    re.captures_iter(sources)
        .filter(|c| reads_deanon(&c[2]))
        .map(|c| c[1].to_string())
        .collect()
}

/// The object expression an identifier is declared with, if it is one.
fn declared_object<'a>(sources: &'a str, ident: &str) -> Option<&'a str> {
    let re = regex::Regex::new(&format!(
        r"\b(?:const|let|var)\s+{}\s*(?::[^=;]+)?=\s*\{{",
        regex::escape(ident)
    ))
    .ok()?;
    let m = re.find(sources)?;
    balanced(sources, m.end() - 1)
}

/// The `attributes` expression of a `startOutboundCall({ … })` argument.
fn attributes_of<'a>(sources: &'a str, argument: &'a str) -> Option<&'a str> {
    static KEY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = KEY.get_or_init(|| {
        regex::Regex::new(r"\battributes\b\s*(:)?\s*").expect("attributes pattern")
    });
    let m = re.captures(argument)?;
    let rest = &argument[m.get(0)?.end()..];
    if m.get(1).is_none() {
        // Shorthand `{ to, flow, attributes }`: the variable of that name.
        return declared_object(sources, "attributes");
    }
    if rest.starts_with('{') {
        let open = argument.len() - rest.len();
        return balanced(argument, open);
    }
    let ident: String = rest.chars().take_while(|c| is_ident_char(*c)).collect();
    if ident.is_empty() {
        // Some other expression: judge it as written.
        return Some(rest.split([',', '}']).next().unwrap_or(rest));
    }
    declared_object(sources, &ident).or(Some(&rest[..ident.len()]))
}

/// Protected values passed in voice call attributes: `.deanon` reads, or
/// identifiers declared from one, inside `startOutboundCall`'s attributes.
pub(crate) fn voice_attribute_leaks(sources: &str) -> Vec<String> {
    let tainted = deanon_identifiers(sources);
    let mut found = Vec::new();
    for (index, _) in sources.match_indices("startOutboundCall(") {
        let open = index + "startOutboundCall".len();
        let Some(argument) = balanced(sources, open) else {
            continue;
        };
        let argument = argument.trim();
        let argument_object = if argument.starts_with('{') {
            balanced(argument, 0).unwrap_or(argument)
        } else {
            // A request built elsewhere: `startOutboundCall(request)`.
            let ident: String = argument.chars().take_while(|c| is_ident_char(*c)).collect();
            match declared_object(sources, &ident) {
                Some(object) => object,
                None => continue,
            }
        };
        let Some(attributes) = attributes_of(sources, argument_object) else {
            continue;
        };
        if reads_deanon(attributes) {
            found.push(".deanon".to_string());
        }
        for ident in &tainted {
            if contains_word(attributes, ident) {
                found.push(ident.clone());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The voice checks for one project.
fn voice_findings(
    project: &str,
    managed: bool,
    has_phi_entities: bool,
    sources: &str,
) -> Vec<LocalFinding> {
    let mut findings = Vec::new();
    if managed {
        let providers = direct_voice_providers(sources);
        if !providers.is_empty() {
            findings.push(LocalFinding {
                severity: Severity::Warning,
                project: project.to_string(),
                check: "voice-provider-direct-in-managed".to_string(),
                subject: providers.join(", "),
                message: format!(
                    "this service runs as a managed instance but calls a voice provider itself ({}) — hosted instances get no vendor credentials; place calls with createVoiceClient(), which dials through the platform's Amazon Connect under the instance's own limits",
                    providers.join(", ")
                ),
            });
        }
    }
    let leaks = voice_attribute_leaks(sources);
    if !leaks.is_empty() {
        findings.push(LocalFinding {
            severity: Severity::Warning,
            project: project.to_string(),
            check: "voice-protected-data".to_string(),
            subject: leaks.join(", "),
            message: format!(
                "a protected value ({}) is passed in startOutboundCall attributes{} — Amazon Connect stores contact attributes in its contact records; pass an id the contact flow looks up instead",
                leaks.join(", "),
                if has_phi_entities {
                    " and this service holds health data (phi)"
                } else {
                    ""
                }
            ),
        });
    }
    findings
}

#[cfg(test)]
mod voice_tests {
    use super::*;

    #[test]
    fn a_managed_service_calling_connect_or_twilio_voice_is_flagged() {
        let sources = "getEnvVar('INSTANCE_HMAC_KEY');\nimport { ConnectClient } from '@aws-sdk/client-connect';\nimport twilio from 'twilio';\nclient.calls.create({ to, from, url });\nconst id = getEnvVar('CONNECT_INSTANCE_ID');";
        let found = voice_findings("clinic", true, false, sources);
        let f = found
            .iter()
            .find(|f| f.check == "voice-provider-direct-in-managed")
            .expect("finding");
        assert_eq!(
            f.subject,
            "@aws-sdk/client-connect, CONNECT_INSTANCE_ID, twilio (voice)"
        );
        // Not managed: an ordinary app may hold its own Connect account.
        assert!(voice_findings("clinic", false, false, sources).is_empty());
    }

    #[test]
    fn twilio_for_sms_and_twilio_keys_are_left_to_managed_provider_credentials() {
        let sources = "import twilio from 'twilio';\nclient.messages.create({ to, body });\ngetEnvVar('TWILIO_AUTH_TOKEN');";
        assert!(direct_voice_providers(sources).is_empty());
        // Similar names are not the key.
        assert!(direct_voice_providers("MY_CONNECT_INSTANCE_ID_OLD").is_empty());
    }

    #[test]
    fn deanon_in_call_attributes_is_flagged_in_every_shape() {
        let inline = "await voice.startOutboundCall({ to: p.phone.deanon, flow: 'appointment_reminder', attributes: { diagnosis: record.diagnosis.deanon } });";
        assert_eq!(voice_attribute_leaks(inline), vec![".deanon"]);

        let via_variable = "const dx = record.diagnosis.deanon;\nconst attrs = { dx, appointmentId: a.id };\nawait voice.startOutboundCall({ to, flow: 'x', attributes: attrs });";
        assert_eq!(voice_attribute_leaks(via_variable), vec!["dx"]);

        let shorthand = "const attributes = { name: patient.name.deanon };\nvoice.startOutboundCall({ to, flow, attributes });";
        assert_eq!(voice_attribute_leaks(shorthand), vec![".deanon"]);

        let request = "const request = { to, flow: 'x', attributes: { n: p.name.deanon() } };\nvoice.startOutboundCall(request);";
        assert_eq!(voice_attribute_leaks(request), vec![".deanon"]);
    }

    #[test]
    fn a_deanon_phone_number_in_to_is_not_a_leak() {
        let sources = "const phone = patient.phone.deanon;\nawait voice.startOutboundCall({ to: phone, flow: 'appointment_reminder', attributes: { appointmentId: appt.id } });\nconst deanonymized = 1;";
        assert!(voice_attribute_leaks(sources).is_empty());
        assert!(voice_attribute_leaks("voice.startOutboundCall({ to, flow });").is_empty());
    }

    #[test]
    fn phi_entities_make_the_leak_critical_in_the_message() {
        let sources = "voice.startOutboundCall({ to, flow, attributes: { d: r.d.deanon } });";
        let f = voice_findings("clinic", false, true, sources);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].check, "voice-protected-data");
        assert!(f[0].message.contains("(phi)"));
        assert!(!voice_findings("clinic", false, false, sources)[0].message.contains("(phi)"));
    }

    #[test]
    fn voice_checks_are_listed() {
        for id in ["voice-provider-direct-in-managed", "voice-protected-data"] {
            assert!(LOCAL_CHECK_IDS.contains(&id));
        }
    }
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
mod payments_tests {
    use super::*;

    #[test]
    fn required_or_direct_stripe_keys_are_found_optional_ones_are_not() {
        let required = "STRIPE_API_KEY: {\n lifetime: Lifetime.Singleton,\n type: string,\n value: getEnvVar('STRIPE_API_KEY') }";
        assert_eq!(managed_stripe_credentials(required), vec!["STRIPE_API_KEY"]);
        let optional = "STRIPE_API_KEY: {\n\t\tlifetime: Lifetime.Singleton,\n\t\ttype: optional(string),\n\t\tvalue: getEnvVar(\"STRIPE_API_KEY\") }\n factory: ({ STRIPE_API_KEY }) => createStripeClient({ Stripe, apiKey: STRIPE_API_KEY })";
        assert!(managed_stripe_credentials(optional).is_empty());
        assert_eq!(
            managed_stripe_credentials("const s = new Stripe(process.env['STRIPE_SECRET_KEY']);"),
            vec!["STRIPE_SECRET_KEY", "new Stripe("]
        );
        // The blueprint's managed branch.
        assert!(managed_stripe_credentials(
            "isManagedInstance() ? createStripeClient({ Stripe }) : new Stripe(STRIPE_API_KEY!)"
        )
        .is_empty());
    }

    #[test]
    fn webhooks_need_a_signature_check() {
        assert!(stripe_webhook_unverified(
            "const event = req.body as Stripe.Event; await handle(event);"
        ));
        assert!(!stripe_webhook_unverified(
            "stripe.webhooks.constructEvent(req.body, req.headers['stripe-signature'], secret)"
        ));
        assert!(!stripe_webhook_unverified(
            "case 'checkout.session.completed': …; verifyPlatformEvent({ method, path, headers, body })"
        ));
        assert!(!stripe_webhook_unverified("await stripe.customers.create({ email })"));
    }

    #[test]
    fn deanon_values_inside_stripe_calls_are_found() {
        let leak = "await this.stripeClient.checkout.sessions.create({\n  mode: 'payment',\n  metadata: { patient: patient.fullName.deanon, visit: visit.id },\n  line_items\n});";
        assert_eq!(stripe_calls_with_protected_data(leak), vec!["checkout.sessions.create"]);
        let description = "stripe.paymentIntents.create({ amount, description: `Visit for ${p.diagnosis.deanon}`, currency })";
        assert_eq!(stripe_calls_with_protected_data(description), vec!["paymentIntents.create"]);
        // An opaque id is fine, and .deanon elsewhere in the call is not metadata.
        let fine = "stripe.customers.create({ email: user.email.deanon, metadata: { userId: user.id } })";
        assert!(stripe_calls_with_protected_data(fine).is_empty());
        let outside = "const n = p.name.deanon;\nstripe.customers.create({ metadata: { ref: p.id } });";
        assert!(stripe_calls_with_protected_data(outside).is_empty());
    }

    fn project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fl-payments-checks-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in files {
            let p = dir.join("clinic").join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        dir
    }

    fn found(dir: &std::path::Path) -> Vec<(String, String)> {
        let out = run_local_checks(dir)
            .unwrap()
            .into_iter()
            .filter(|f| f.check.starts_with("payments") || f.check.starts_with("stripe"))
            .map(|f| (f.check, f.subject))
            .collect();
        let _ = std::fs::remove_dir_all(dir);
        out
    }

    #[test]
    fn a_managed_service_with_a_required_stripe_key_is_flagged() {
        let dir = project(
            "library-events",
            &[("services/webhook.service.ts", "handle(event: Stripe.Event) { switch (event.type) { case 'checkout.session.completed': } }")],
        );
        assert!(found(&dir).is_empty(), "a library is not the webhook receiver");
        let dir = project(
            "unverified-route",
            &[("api/controllers/webhook.controller.ts", "const event = req.body as Stripe.Event; await service.handle(event);")],
        );
        assert_eq!(found(&dir), vec![("stripe-webhook-unverified".to_string(), "webhook".to_string())]);
        let dir = project(
            "managed-key",
            &[(
                "registrations.ts",
                "getEnvVar('INSTANCE_HMAC_KEY'); STRIPE_API_KEY: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('STRIPE_API_KEY') }",
            )],
        );
        assert_eq!(
            found(&dir),
            vec![("payments-stripe-keys-in-managed".to_string(), "STRIPE_API_KEY".to_string())]
        );
        // Not managed: a key is how Stripe is reached.
        let dir = project(
            "unmanaged-key",
            &[(
                "registrations.ts",
                "STRIPE_API_KEY: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('STRIPE_API_KEY') }",
            )],
        );
        assert!(found(&dir).is_empty());
    }

    #[test]
    fn protected_data_is_critical_only_when_the_service_holds_phi() {
        let service = "await stripe.customers.create({ metadata: { dob: patient.dob.deanon } });";
        let phi = project(
            "phi",
            &[
                ("domain/services/pay.service.ts", service),
                (
                    "persistence/entities/patient.entity.ts",
                    "export const P = defineComplianceEntity({ name: 'P', properties: { dob: fp.string().compliance('phi') } });",
                ),
            ],
        );
        assert_eq!(
            found(&phi),
            vec![("payments-protected-data".to_string(), "phi: customers.create".to_string())]
        );
        let pii = project("pii", &[("domain/services/pay.service.ts", service)]);
        assert_eq!(
            found(&pii),
            vec![("payments-protected-data".to_string(), "customers.create".to_string())]
        );
    }

    #[test]
    fn a_keyed_stripe_client_outside_managed_mode_is_not_undeclared_payments() {
        let write = |name: &str, registrations: &str| {
            let root = std::env::temp_dir().join(format!("fl-payments-wiring-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
            std::fs::create_dir_all(root.join("src/modules/billing")).unwrap();
            std::fs::write(
                root.join(".forklaunch/manifest.toml"),
                "app_name = \"demo\"\n\n[[projects]]\nname = \"billing\"\n[projects.resources]\ncache = \"redis\"\n",
            )
            .unwrap();
            std::fs::write(root.join("src/modules/billing/registrations.ts"), registrations).unwrap();
            let checks: Vec<String> = run_local_checks(&root.join("src/modules"))
                .unwrap()
                .into_iter()
                .filter(|f| f.check == "capability-wiring")
                .map(|f| f.subject)
                .collect();
            let _ = std::fs::remove_dir_all(&root);
            checks
        };
        assert!(write("plain", "StripeClient: { factory: ({ STRIPE_API_KEY }) => new Stripe(STRIPE_API_KEY) }").is_empty());
        assert_eq!(
            write(
                "managed",
                "getEnvVar('INSTANCE_HMAC_KEY'); StripeClient: { factory: () => createStripeClient({ Stripe }) }"
            ),
            vec!["payments"]
        );
    }

    #[test]
    fn payments_checks_are_listed() {
        for id in [
            "payments-stripe-keys-in-managed",
            "stripe-webhook-unverified",
            "payments-protected-data",
        ] {
            assert!(LOCAL_CHECK_IDS.contains(&id), "{id} missing from LOCAL_CHECK_IDS");
        }
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

#[cfg(test)]
mod sms_tests {
    use super::*;

    fn run(name: &str, files: &[(&str, &str)], phi: bool) -> Vec<LocalFinding> {
        let dir = std::env::temp_dir().join(format!("fl-sms-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        let proj = dir.join("notify");
        for (rel, text) in files {
            let path = proj.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        if phi {
            let entities = proj.join("persistence/entities");
            std::fs::create_dir_all(&entities).unwrap();
            std::fs::write(
                entities.join("patient.entity.ts"),
                "export const P = defineComplianceEntity({ name: 'P', properties: { id: fp.uuid().primary().compliance('none'), diagnosis: fp.text().compliance('phi') } });",
            )
            .unwrap();
        }
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        findings
    }

    #[test]
    fn a_managed_service_importing_an_sms_sdk_is_flagged_once_per_subject() {
        let findings = run(
            "sdk",
            &[(
                "registrations.ts",
                "import twilio from 'twilio';\nimport { PinpointSMSVoiceV2Client } from '@aws-sdk/client-pinpoint-sms-voice-v2';\nconst k = getEnvVar('INSTANCE_HMAC_KEY'); const t = getEnvVar('TWILIO_AUTH_TOKEN');",
            )],
            false,
        );
        let sdk = findings
            .iter()
            .find(|f| f.check == "sms-provider-direct-in-managed")
            .unwrap_or_else(|| panic!("{findings:?}"));
        assert_eq!(sdk.subject, "@aws-sdk/client-pinpoint-sms-voice-v2, twilio");
        // The key is the credentials check's, not repeated here.
        let keys = findings
            .iter()
            .find(|f| f.check == "managed-provider-credentials")
            .unwrap();
        assert_eq!(keys.subject, "TWILIO_AUTH_TOKEN");
        assert!(!sdk.subject.contains("TWILIO_AUTH_TOKEN"));
    }

    #[test]
    fn sdk_imports_outside_managed_mode_and_sns_without_phones_are_fine() {
        let unmanaged = run("unmanaged", &[("registrations.ts", "import twilio from 'twilio';")], false);
        assert!(!unmanaged.iter().any(|f| f.check == "sms-provider-direct-in-managed"));
        assert!(direct_sms_providers(
            "import { SNSClient } from '@aws-sdk/client-sns'; publish({ TopicArn })"
        )
        .is_empty());
        assert_eq!(
            direct_sms_providers("import { SNSClient } from '@aws-sdk/client-sns'; ({ PhoneNumber })"),
            vec!["@aws-sdk/client-sns"]
        );
        assert_eq!(
            provider_credentials_read("VONAGE_API_SECRET MESSAGEBIRD_ACCESS_KEY"),
            vec!["MESSAGEBIRD_ACCESS_KEY", "VONAGE_API_SECRET"]
        );
    }

    #[test]
    fn deanon_in_an_sms_body_is_flagged_critical_with_phi() {
        let source = "export async function remind(p: Patient) {\n  const note = `Your result: ${p.diagnosis.deanon}`;\n  await this.smsClient.send({ to: p.phone.deanon, body: note });\n}\n";
        let findings = run("phi", &[("domain/services/remind.service.ts", source)], true);
        let f = findings
            .iter()
            .find(|f| f.check == "sms-protected-data")
            .unwrap_or_else(|| panic!("{findings:?}"));
        assert_eq!(f.subject, "domain/services/remind.service.ts: smsClient.send (line 3)");
        assert!(f.message.starts_with("health data"));
        let card = crate::core::report_card::build_local_report_card("app", 1, &findings, "t".into());
        let json = serde_json::to_string(&card).unwrap();
        assert!(json.contains("\"severity\":\"critical\""), "{json}");

        let no_phi = run("nophi", &[("domain/services/remind.service.ts", source)], false);
        let f = no_phi.iter().find(|f| f.check == "sms-protected-data").unwrap();
        assert!(!f.message.starts_with("health data"));
    }

    #[test]
    fn protected_data_detection_follows_the_body_only() {
        // The number is rightly plaintext; the text is neutral.
        assert!(sms_sends_with_protected_data(
            "await sms.send({ to: user.phone.deanon, body: 'Your visit is confirmed' });"
        )
        .is_empty());
        // Direct, shorthand and vendor forms.
        assert_eq!(
            sms_sends_with_protected_data("sms.send({ to, body: `Hi ${user.name.deanon}` });").len(),
            1
        );
        assert_eq!(
            sms_sends_with_protected_data(
                "const body = user.ssn.deanon;\nawait smsClient.send({ to: x, body });"
            ),
            vec!["smsClient.send (line 2)"]
        );
        assert_eq!(
            sms_sends_with_protected_data(
                "import twilio from 'twilio';\nconst m = r.notes.deanon; const text = m + '!';\nclient.messages.create({ to, from, body: text });"
            )
            .len(),
            1
        );
        assert_eq!(
            sms_sends_with_protected_data(
                "new SendTextMessageCommand({ DestinationPhoneNumber: to, MessageBody: r.notes.deanon })"
            )
            .len(),
            1
        );
        // Not an SMS: email, or messages.create without twilio.
        assert!(sms_sends_with_protected_data("mailer.send({ to, body: r.notes.deanon });").is_empty());
        assert!(sms_sends_with_protected_data("openai.messages.create({ body: r.notes.deanon });").is_empty());
        assert!(sms_sends_with_protected_data("new PublishCommand({ TopicArn, Message: r.notes.deanon })").is_empty());
        // A variable named like a tainted one is not it.
        assert!(sms_sends_with_protected_data(
            "const secret = r.notes.deanon; sms.send({ to, body: secretary });"
        )
        .is_empty());
    }

    #[test]
    fn sms_checks_are_listed_and_scored() {
        for id in ["sms-provider-direct-in-managed", "sms-protected-data"] {
            assert!(LOCAL_CHECK_IDS.contains(&id));
        }
    }
}

#[cfg(test)]
mod whatsapp_tests {
    use super::*;

    fn service(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fl-whatsapp-checks-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in files {
            let full = dir.join("svc").join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        dir
    }

    const MANAGED: &str = "const g = getEnvVar('PLATFORM_GATEWAY_URL'); const k = getEnvVar('INSTANCE_HMAC_KEY');";

    #[test]
    fn direct_meta_access_is_flagged_in_a_managed_service() {
        assert_eq!(
            direct_whatsapp_access(
                "fetch(`https://graph.facebook.com/v20.0/${id}/messages`, { headers: { authorization: `Bearer ${process.env.WHATSAPP_TOKEN}` } })"
            ),
            vec!["WHATSAPP_TOKEN", "graph.facebook.com"]
        );
        assert_eq!(
            direct_whatsapp_access("import { SocialMessagingClient } from '@aws-sdk/client-socialmessaging'; const s = getEnvVar('META_ACCESS_TOKEN');"),
            vec!["@aws-sdk/client-socialmessaging", "META_ACCESS_TOKEN"]
        );
        // The gateway client, and unrelated META_ names, are fine.
        assert!(direct_whatsapp_access("createWhatsAppClient(); const m = META_TITLE; const d = SERVICE_METADATA;").is_empty());

        let dir = service(
            "direct",
            &[("registrations.ts", &format!("{MANAGED} const t = getEnvVar('WHATSAPP_ACCESS_TOKEN');"))],
        );
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let f = findings
            .iter()
            .find(|f| f.check == "whatsapp-provider-direct-in-managed")
            .unwrap_or_else(|| panic!("expected the finding, got {findings:?}"));
        assert_eq!(f.subject, "WHATSAPP_ACCESS_TOKEN");
    }

    #[test]
    fn an_unmanaged_service_calling_meta_is_not_a_managed_finding() {
        let dir = service("unmanaged", &[("registrations.ts", "getEnvVar('WHATSAPP_TOKEN')")]);
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!findings.iter().any(|f| f.check == "whatsapp-provider-direct-in-managed"));
    }

    #[test]
    fn deanon_values_reaching_a_whatsapp_send_are_found() {
        let direct = "const whatsapp = ci.resolve(tokens.WhatsAppClient);\nawait whatsapp.sendText({ to: patient.phone.deanon, body: 'hi' });";
        assert_eq!(whatsapp_protected_sends(direct), vec!["2: whatsapp.sendText"]);

        let through_variables = "import { createWhatsAppClient } from '@forklaunch/core/http';
const diagnosis = record.diagnosis.deanon;
const note = `Your result: ${diagnosis}`;
await this.whatsappClient.sendTemplate({
  to,
  template: 'result_ready',
  language: 'en_US',
  components: [{ type: 'body', parameters: [{ type: 'text', text: note }] }]
});";
        assert_eq!(whatsapp_protected_sends(through_variables), vec!["4: whatsappClient.sendTemplate"]);

        let sdk = "await client.send(new SendWhatsAppMessageCommand({ originationPhoneNumberId, message: encode(p.name.deanon), metaApiVersion: 'v20.0' }));";
        assert_eq!(whatsapp_protected_sends(sdk), vec!["1: SendWhatsAppMessageCommand"]);
    }

    #[test]
    fn sends_without_plaintext_and_other_channels_are_not_flagged() {
        // A reminder that carries nothing protected.
        let clean = "const name = patient.name.deanon;\nlog(name);\nawait whatsapp.sendTemplate({ to: phone, template: 'appointment_reminder', language: 'en_US' });";
        assert!(whatsapp_protected_sends(clean).is_empty());
        // Same method name on an SMS client: another feature's check.
        let sms = "// whatsapp later\nawait sms.sendText({ to: p.phone.deanon, body: 'x' });";
        assert!(whatsapp_protected_sends(sms).is_empty());
        // `.name` on another object is not the tainted `name` variable.
        let property = "const name = p.name.deanon;\nawait whatsapp.sendText({ to, body: user.name });";
        assert!(whatsapp_protected_sends(property).is_empty());
    }

    #[test]
    fn protected_data_is_marked_phi_when_entities_hold_health_data() {
        let entity = "import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
export const PatientEntity = defineComplianceEntity({
  name: 'Patient',
  properties: {
    diagnosis: fp.string().compliance('phi')
  }
});";
        let controller = "await whatsapp.sendText({ to: p.phone, body: p.diagnosis.deanon });";
        let dir = service(
            "phi",
            &[
                ("persistence/entities/patient.entity.ts", entity),
                ("api/controllers/notify.controller.ts", controller),
            ],
        );
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let f = findings
            .iter()
            .find(|f| f.check == "whatsapp-protected-data")
            .unwrap_or_else(|| panic!("expected the finding, got {findings:?}"));
        assert_eq!(f.subject, "api/controllers/notify.controller.ts:1: whatsapp.sendText");
        assert!(f.message.starts_with(WHATSAPP_PHI_MARKER), "{}", f.message);

        let dir = service("nophi", &[("api/controllers/notify.controller.ts", controller)]);
        let findings = run_local_checks(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let f = findings.iter().find(|f| f.check == "whatsapp-protected-data").unwrap();
        assert!(!f.message.contains(WHATSAPP_PHI_MARKER));
    }

    #[test]
    fn whatsapp_checks_are_listed() {
        for id in ["whatsapp-provider-direct-in-managed", "whatsapp-protected-data"] {
            assert!(LOCAL_CHECK_IDS.contains(&id), "{id} missing from LOCAL_CHECK_IDS");
        }
    }
}
