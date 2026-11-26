//! Explicit partial disclosures are not de-identification certifications. Default
//! redaction remains irreversible; token disclosure is random, never derived
//! from plaintext or equality indexes. Public metadata is authenticated without
//! decrypting stored values, including across configured key rotation.
use super::{
    EnvelopeKey, FrameworkError, FrameworkResult, ProtectedContext, ProtectedField,
    ProtectedMember, Serializer, unavailable,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisclosurePolicy {
    #[default]
    Redact,
    RandomToken,
    Year,
    /// Three ASCII digits, suppressing the TS compatibility list. This historic
    /// list is not an assertion of current geographic/privacy policy compliance.
    Zip3,
    Last4,
}
/// Only authenticated protected fields can create this value. Selecting partial
/// disclosure is an application policy decision; sinks must not equate its type
/// with legal anonymity. Debug suppresses even the selected projection.
pub struct DisclosureValue {
    policy: DisclosurePolicy,
    value: String,
}
impl std::fmt::Debug for DisclosureValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DisclosureValue([redacted])")
    }
}
impl Serialize for DisclosureValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.value)
    }
}
impl DisclosureValue {
    #[must_use]
    pub const fn policy(&self) -> DisclosurePolicy {
        self.policy
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}
impl ProtectedMember {
    /// Opt-in disclosure policy. Existing ciphertext requires an explicit audited
    /// reclassification migration before a non-redacted projection is available.
    #[must_use]
    pub const fn with_disclosure(mut self, policy: DisclosurePolicy) -> Self {
        self.disclosure = policy;
        self
    }
}
impl ProtectedField {
    /// Authenticate stored projection metadata without decrypting or issuing a
    /// plaintext access audit. Authorization remains the caller's responsibility.
    /// # Errors
    /// Rejects legacy rows without a projection, policy/context substitution,
    /// malformed metadata and missing/retired authentication keys.
    pub fn disclosed(
        &self,
        key: &EnvelopeKey,
        context: ProtectedContext,
    ) -> FrameworkResult<DisclosureValue> {
        if self.member.disclosure == DisclosurePolicy::Redact {
            return Ok(DisclosureValue {
                policy: DisclosurePolicy::Redact,
                value: "[redacted]".into(),
            });
        }
        let metadata = parse(&self.ciphertext)?.ok_or_else(migration_required)?;
        verify(key, context, &self.member, &metadata)?;
        Ok(DisclosureValue {
            policy: metadata.policy,
            value: metadata.value,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    policy: DisclosurePolicy,
    value: String,
    tag: String,
    ciphertext: String,
}
fn parse(storage: &str) -> FrameworkResult<Option<Metadata>> {
    if storage.len() > 2_097_152 {
        return Err(unavailable());
    }
    let Some(encoded) = storage.strip_prefix("pd1.") else {
        return Ok(None);
    };
    let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| unavailable())?;
    let value: Metadata = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if value.policy == DisclosurePolicy::Redact
        || value.value.len() > 128
        || value.tag.len() > 256
        || !value.ciphertext.starts_with("v3.")
    {
        return Err(unavailable());
    }
    Ok(Some(value))
}
pub(super) fn validate_storage(storage: &str) -> FrameworkResult<()> {
    parse(storage).map(|_| ())
}
fn binding(member: &ProtectedMember, metadata: &Metadata) -> FrameworkResult<Vec<u8>> {
    serde_json::to_vec(&(
        "protected-disclosure-v1",
        member.entity,
        member.field,
        &member.purpose,
        format!("{:?}", member.level),
        metadata.policy,
        &metadata.value,
        &metadata.ciphertext,
    ))
    .map_err(|_| unavailable())
}
fn verify(
    key: &EnvelopeKey,
    context: ProtectedContext,
    member: &ProtectedMember,
    metadata: &Metadata,
) -> FrameworkResult<()> {
    if metadata.policy != member.disclosure {
        return Err(unavailable());
    }
    key.verify_public_metadata(
        context.tenant,
        context.resource,
        &member.purpose,
        &binding(member, metadata)?,
        &metadata.tag,
    )
    .map_err(|_| unavailable())
}
pub(super) fn authenticated_inner<'a>(
    key: &EnvelopeKey,
    context: ProtectedContext,
    member: &ProtectedMember,
    storage: &'a str,
) -> FrameworkResult<std::borrow::Cow<'a, str>> {
    match parse(storage)? {
        None => Ok(std::borrow::Cow::Borrowed(storage)),
        Some(metadata) => {
            verify(key, context, member, &metadata)?;
            Ok(std::borrow::Cow::Owned(metadata.ciphertext))
        }
    }
}
pub(super) fn seal(
    key: &EnvelopeKey,
    context: ProtectedContext,
    member: &ProtectedMember,
    plaintext: &[u8],
    ciphertext: String,
    retained: Option<&DisclosureValue>,
) -> FrameworkResult<String> {
    if member.disclosure == DisclosurePolicy::Redact {
        return Ok(ciphertext);
    }
    let value = match retained {
        Some(value) if value.policy == member.disclosure => value.value.clone(),
        Some(_) => return Err(unavailable()),
        None => project(member.disclosure, plaintext)?,
    };
    let mut metadata = Metadata {
        policy: member.disclosure,
        value,
        tag: String::new(),
        ciphertext,
    };
    metadata.tag = key
        .sign_public_metadata(
            context.tenant,
            context.resource,
            &member.purpose,
            &binding(member, &metadata)?,
        )
        .map_err(|_| unavailable())?;
    let stored = format!(
        "pd1.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&metadata).map_err(|_| unavailable())?)
    );
    validate_storage(&stored)?;
    Ok(stored)
}
fn project(policy: DisclosurePolicy, plaintext: &[u8]) -> FrameworkResult<String> {
    if policy == DisclosurePolicy::RandomToken {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| unavailable())?;
        return Ok(format!("tok_{}", URL_SAFE_NO_PAD.encode(random)));
    }
    let value = std::str::from_utf8(plaintext).map_err(|_| unavailable())?;
    Ok(match policy {
        DisclosurePolicy::Year => value
            .as_bytes()
            .windows(4)
            .find(|v| v.iter().all(u8::is_ascii_digit))
            .map_or_else(
                || "[redacted]".into(),
                |v| String::from_utf8(v.to_vec()).expect("ASCII digits"),
            ),
        DisclosurePolicy::Zip3 => {
            let digits: String = value.chars().filter(char::is_ascii_digit).take(3).collect();
            if digits.len() < 3
                || [
                    "036", "059", "063", "102", "203", "556", "692", "790", "821", "823", "830",
                    "831", "878", "879", "884", "890", "893",
                ]
                .contains(&digits.as_str())
            {
                "000".into()
            } else {
                digits
            }
        }
        DisclosurePolicy::Last4 => {
            let last: String = value
                .chars()
                .rev()
                .filter(char::is_ascii_digit)
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if last.len() < 4 {
                "[redacted]".into()
            } else {
                format!("•••• {last}")
            }
        }
        DisclosurePolicy::Redact => "[redacted]".into(),
        DisclosurePolicy::RandomToken => {
            unreachable!("random token handled before plaintext parsing")
        }
    })
}
fn migration_required() -> FrameworkError {
    FrameworkError::new(
        "protected_disclosure_migration_required",
        "Stored value requires an explicit audited disclosure migration",
    )
}

pub(super) fn stored_key_id(storage: &str) -> Option<String> {
    let inner = match parse(storage).ok()? {
        Some(value) => value.ciphertext,
        None => storage.to_owned(),
    };
    let mut parts = inner.split('.');
    match parts.next()? {
        "v2" | "v3" => parts.next().map(str::to_owned),
        _ => None,
    }
}
