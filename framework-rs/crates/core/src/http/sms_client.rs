//! Platform-held SMS with TypeScript-compatible GSM/UCS segment accounting.
use crate::http::instance_gateway::{InstanceGatewayClient, InstanceGatewayRequestError};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
pub const SMS_MAX_SEGMENTS: usize = 10;
pub type SmsRequestError = InstanceGatewayRequestError;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SmsPurpose {
    Transactional,
    Promotional,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SmsSendRequest {
    pub to: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<SmsPurpose>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsSendResult {
    pub message_id: String,
    pub segments: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SmsEncoding {
    #[serde(rename = "GSM-7")]
    Gsm7,
    #[serde(rename = "UCS-2")]
    Ucs2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SmsSegments {
    pub encoding: SmsEncoding,
    pub units: usize,
    pub segments: usize,
}
#[must_use]
pub fn is_e164(phone: &str) -> bool {
    let bytes = phone.as_bytes();
    (8..=16).contains(&bytes.len())
        && bytes[0] == b'+'
        && (b'1'..=b'9').contains(&bytes[1])
        && bytes[2..].iter().all(u8::is_ascii_digit)
}
/// The GSM extension set counts as two septets; UCS-2 counts UTF-16 code units.
#[must_use]
pub fn sms_segments(body: &str) -> SmsSegments {
    const BASIC: &str = "@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà";
    const EXTENDED: &str = "^{}\\[~]|€\u{c}";
    let mut septets = 0;
    for ch in body.chars() {
        if BASIC.contains(ch) {
            septets += 1;
        } else if EXTENDED.contains(ch) {
            septets += 2;
        } else {
            let units = body.encode_utf16().count();
            return SmsSegments {
                encoding: SmsEncoding::Ucs2,
                units,
                segments: if units <= 70 { 1 } else { units.div_ceil(67) },
            };
        }
    }
    SmsSegments {
        encoding: SmsEncoding::Gsm7,
        units: septets,
        segments: if septets <= 160 {
            1
        } else {
            septets.div_ceil(153)
        },
    }
}
/// Matches ECMAScript trim whitespace, including BOM and excluding U+0085.
#[must_use]
pub fn nonempty_text(body: &str) -> bool {
    body.chars().any(|ch| !is_ecmascript_whitespace(ch))
}
pub(crate) fn is_ecmascript_whitespace(ch: char) -> bool {
    matches!(ch, '\u{9}'..='\u{d}'|'\u{20}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
/// # Errors
/// Rejects invalid phone numbers, whitespace-only text and overlong messages.
pub fn validate_sms_send(request: &SmsSendRequest) -> Result<(), SmsRequestError> {
    if !is_e164(&request.to) {
        return Err(SmsRequestError::validation("sms_invalid_recipient"));
    }
    if !nonempty_text(&request.body) {
        return Err(SmsRequestError::validation("sms_empty_body"));
    }
    if sms_segments(&request.body).segments > SMS_MAX_SEGMENTS {
        return Err(SmsRequestError::validation("sms_too_many_segments"));
    }
    Ok(())
}
#[derive(Debug, Clone)]
pub struct SmsClient {
    gateway: Option<InstanceGatewayClient>,
}
impl SmsClient {
    #[must_use]
    pub const fn new(gateway: InstanceGatewayClient) -> Self {
        Self {
            gateway: Some(gateway),
        }
    }
    /// Allows dependency registration before managed settings are available;
    /// sends validate their input then refuse until a configured client is used.
    #[must_use]
    pub const fn deferred() -> Self {
        Self { gateway: None }
    }
    #[must_use]
    pub const fn managed(&self) -> bool {
        self.gateway.is_some()
    }
    /// # Errors
    /// Rejects invalid input/unmanaged mode, gateway refusals and malformed DTOs.
    pub async fn send(&self, request: &SmsSendRequest) -> Result<SmsSendResult, SmsRequestError> {
        validate_sms_send(request)?;
        let gateway = self
            .gateway
            .as_ref()
            .ok_or_else(|| SmsRequestError::validation("sms_not_managed"))?;
        gateway.json(Method::POST,&["sms","send"],Some(&json!({"to":request.to,"body":request.body,"purpose":request.purpose.unwrap_or(SmsPurpose::Transactional)}))).await
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsDeliveredData {
    pub message_id: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsFailedData {
    pub message_id: String,
    pub to: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsReceivedData {
    pub from: String,
    pub body: String,
    pub received_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsOptedOutData {
    pub phone: String,
    pub opted_out_at: String,
}
