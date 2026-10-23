//! Platform-held email with mailbox, recipient, subject, body and tag limits.
use crate::http::instance_gateway::InstanceGatewayClient;
use crate::http::instance_gateway::InstanceGatewayRequestError;
use crate::http::sms_client::is_ecmascript_whitespace;
use crate::http::sms_client::nonempty_text;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub type EmailRequestError = InstanceGatewayRequestError;
pub const EMAIL_MAX_RECIPIENTS: usize = 50;
pub const EMAIL_MAX_ADDRESS_LENGTH: usize = 254;
pub const EMAIL_MAX_SUBJECT_LENGTH: usize = 998;
pub const EMAIL_MAX_BODY_BYTES: usize = 512 * 1024;
pub const EMAIL_MAX_TAGS: usize = 10;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum EmailAddresses {
    One(String),
    Many(Vec<String>),
}
impl EmailAddresses {
    fn addresses(&self) -> Vec<&str> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values.iter().map(String::as_str).collect(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailSendRequest {
    pub to: EmailAddresses,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<EmailAddresses>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailSendResult {
    pub message_id: String,
}
#[must_use]
pub fn is_email_address(address: &str) -> bool {
    if address.encode_utf16().count() > EMAIL_MAX_ADDRESS_LENGTH {
        return false;
    }
    let Some((local, domain)) = address.split_once('@') else {
        return false;
    };
    if local.is_empty()
        || local.chars().any(|ch| {
            is_ecmascript_whitespace(ch)
                || matches!(
                    ch,
                    '@' | '<' | '>' | '(' | ')' | '[' | ']' | '\\' | ',' | ';' | ':' | '"'
                )
        })
    {
        return false;
    }
    let labels: Vec<_> = domain.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            let bytes = label.as_bytes();
            !bytes.is_empty()
                && bytes[0].is_ascii_alphanumeric()
                && bytes[bytes.len() - 1].is_ascii_alphanumeric()
                && bytes
                    .iter()
                    .all(|v| v.is_ascii_alphanumeric() || *v == b'-')
        })
}
fn validate_addresses(addresses: &EmailAddresses) -> Result<Vec<&str>, EmailRequestError> {
    let addresses = addresses.addresses();
    if addresses.is_empty()
        || addresses.len() > EMAIL_MAX_RECIPIENTS
        || !addresses.iter().all(|address| is_email_address(address))
    {
        return Err(invalid("email_invalid_addresses"));
    }
    Ok(addresses)
}
/// Normalizes scalar recipients into arrays and excludes empty tag maps.
/// # Errors
/// Rejects invalid recipients, subjects, missing/oversized bodies and tag rules.
pub fn validate_email_send_request(request: &EmailSendRequest) -> Result<Value, EmailRequestError> {
    let to = validate_addresses(&request.to)?;
    let reply_to = request
        .reply_to
        .as_ref()
        .map(validate_addresses)
        .transpose()?;
    if !nonempty_text(&request.subject)
        || request.subject.contains(['\r', '\n'])
        || request.subject.encode_utf16().count() > EMAIL_MAX_SUBJECT_LENGTH
    {
        return Err(invalid("email_invalid_subject"));
    }
    let text = request.text.as_deref().unwrap_or_default();
    let html = request.html.as_deref().unwrap_or_default();
    if text.is_empty() && html.is_empty() {
        return Err(invalid("email_missing_body"));
    }
    if text.len().saturating_add(html.len()) > EMAIL_MAX_BODY_BYTES {
        return Err(invalid("email_body_too_large"));
    }
    let mut body = json!({"to":to,"subject":request.subject});
    if let Some(text) = &request.text {
        body["text"] = json!(text);
    }
    if let Some(html) = &request.html {
        body["html"] = json!(html);
    }
    if let Some(reply_to) = reply_to {
        body["replyTo"] = json!(reply_to);
    }
    if let Some(tags) = &request.tags {
        if tags.len() > EMAIL_MAX_TAGS {
            return Err(invalid("email_too_many_tags"));
        }
        for (name, value) in tags {
            if !tag_part(name)
                || !tag_part(value)
                || name.starts_with("fl-")
                || name.starts_with("ses:")
            {
                return Err(invalid("email_invalid_tag"));
            }
        }
        if !tags.is_empty() {
            body["tags"] = json!(tags);
        }
    }
    Ok(body)
}
fn tag_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'_' | b'-'))
}
fn invalid(code: &str) -> EmailRequestError {
    let mut error = EmailRequestError::validation(code);
    error.status = Some(400);
    error
}
#[derive(Debug, Clone)]
pub struct EmailClient {
    gateway: InstanceGatewayClient,
}
impl EmailClient {
    #[must_use]
    pub const fn new(gateway: InstanceGatewayClient) -> Self {
        Self { gateway }
    }
    /// # Errors
    /// Rejects invalid input, gateway refusals and malformed response DTOs.
    pub async fn send(
        &self,
        request: &EmailSendRequest,
    ) -> Result<EmailSendResult, EmailRequestError> {
        let body = validate_email_send_request(request)?;
        self.gateway
            .json(Method::POST, &["email", "send"], Some(&body))
            .await
    }
    /// # Errors
    /// Rejects invalid addresses, gateway refusals and malformed responses.
    pub async fn suppressed(&self, address: &str) -> Result<bool, EmailRequestError> {
        #[derive(Deserialize)]
        struct Suppression {
            suppressed: Value,
        }
        if !is_email_address(address) {
            return Err(invalid("email_invalid_address"));
        }
        let suppression: Suppression = self
            .gateway
            .json(
                Method::GET,
                &["email", "suppressions", &address.to_lowercase()],
                None,
            )
            .await?;
        Ok(suppression.suppressed == Value::Bool(true))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailEventData {
    pub message_id: String,
    pub recipients: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounce_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounce_sub_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permanent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_type: Option<String>,
}
