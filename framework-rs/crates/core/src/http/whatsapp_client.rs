//! Managed `WhatsApp` templates and text through the instance-signed gateway.
use crate::http::instance_gateway::InstanceGatewayClient;
use crate::http::instance_gateway::InstanceGatewayRequestError;
use crate::http::sms_client::is_e164;
use crate::http::sms_client::nonempty_text;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub type WhatsAppRequestError = InstanceGatewayRequestError;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WhatsAppTemplateComponent {
    #[serde(rename = "type")]
    pub component_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<BTreeMap<String, Value>>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendTemplateRequest {
    pub to: String,
    pub template: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<WhatsAppTemplateComponent>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendTextRequest {
    pub to: String,
    pub body: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    pub message_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageTemplate {
    pub name: String,
    pub language: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}
#[derive(Debug, Clone)]
pub struct WhatsAppClient {
    gateway: InstanceGatewayClient,
}
impl WhatsAppClient {
    #[must_use]
    pub const fn new(gateway: InstanceGatewayClient) -> Self {
        Self { gateway }
    }
    /// # Errors
    /// Rejects invalid recipients/missing template fields and gateway failures.
    pub async fn send_template(
        &self,
        request: &SendTemplateRequest,
    ) -> Result<SentMessage, WhatsAppRequestError> {
        if !is_e164(&request.to) {
            return Err(invalid("whatsapp_invalid_recipient"));
        }
        if request.template.is_empty() || request.language.is_empty() {
            return Err(invalid("whatsapp_missing_template"));
        }
        let mut body = json!({"to":request.to,"type":"template","template":request.template,"language":request.language});
        if let Some(components) = &request.components {
            body["components"] = json!(components);
        }
        self.gateway
            .json(Method::POST, &["whatsapp", "messages"], Some(&body))
            .await
    }
    /// # Errors
    /// Rejects invalid recipients/empty text and preserves gateway refusal status.
    pub async fn send_text(
        &self,
        request: &SendTextRequest,
    ) -> Result<SentMessage, WhatsAppRequestError> {
        if !is_e164(&request.to) {
            return Err(invalid("whatsapp_invalid_recipient"));
        }
        if !nonempty_text(&request.body) {
            return Err(invalid("whatsapp_empty_body"));
        }
        self.gateway
            .json(
                Method::POST,
                &["whatsapp", "messages"],
                Some(&json!({"to":request.to,"type":"text","body":request.body})),
            )
            .await
    }
    /// # Errors
    /// Rejects gateway failures and malformed template listings.
    pub async fn templates(&self) -> Result<Vec<MessageTemplate>, WhatsAppRequestError> {
        #[derive(Deserialize)]
        struct Listing {
            templates: Vec<MessageTemplate>,
        }
        let listing: Listing = self
            .gateway
            .json(Method::GET, &["whatsapp", "templates"], None)
            .await?;
        Ok(listing.templates)
    }
}
fn invalid(code: &str) -> WhatsAppRequestError {
    let mut error = WhatsAppRequestError::validation(code);
    error.status = Some(400);
    error
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppReceivedEvent {
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub message_type: String,
    pub received_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppStatusEvent {
    pub message_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}
