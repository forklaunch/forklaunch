//! Stripe `PaymentIntents` and direct Connect charges backed by the atomic
//! ecommerce store. Client secrets are response-only and never persisted.
#![forbid(unsafe_code)]
#[cfg(feature = "postgres")]
pub mod bound_reads;
pub mod context;
#[cfg(feature = "http")]
pub mod http;
pub mod reconciliation;
#[cfg(feature = "postgres")]
pub mod scoped_cancellation;
#[cfg(feature = "postgres")]
pub mod scoped_capture;
pub mod tax;
pub mod webhook;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::http::provider_http::provider_status_error;
use forklaunch_ecommerce_base::{BaseEcommerceService, store::EcommerceStore};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentDto, PaymentService,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct StripeClient {
    http: ProviderHttpClient,
    secret: Arc<Zeroizing<String>>,
    generation: Arc<()>,
}
impl std::fmt::Debug for StripeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeClient")
            .field("http", &self.http)
            .finish_non_exhaustive()
    }
}
/// `client_secret` is intentionally redacted from Debug and excluded from
/// persistence. Avoid logging provider response bodies or serialized checkout DTOs.
#[derive(Clone, Deserialize)]
pub struct PaymentIntent {
    pub id: String,
    pub client_secret: Option<String>,
}
impl std::fmt::Debug for PaymentIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaymentIntent")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl StripeClient {
    /// # Errors
    /// Rejects empty credentials and invalid transport configuration.
    pub fn new(secret: String) -> BlueprintResult<Self> {
        Self::with_transport(
            secret,
            ProviderHttpClient::new(
                "https://api.stripe.com",
                Duration::from_secs(15),
                1024 * 1024,
            )?,
        )
    }
    /// Allows a trusted TLS proxy or loopback fixture with bounded transport.
    /// # Errors
    /// Rejects empty/control-containing credentials.
    pub fn with_transport(secret: String, http: ProviderHttpClient) -> BlueprintResult<Self> {
        if secret.is_empty() || secret.bytes().any(|b| b.is_ascii_control()) {
            return Err(config_error());
        }
        Ok(Self {
            http,
            secret: Arc::new(Zeroizing::new(secret)),
            generation: Arc::new(()),
        })
    }
    /// # Errors
    /// Returns sanitized provider/network/shape errors.
    pub async fn create_intent(
        &self,
        request: &CreatePaymentDto,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<PaymentIntent> {
        let mut fields = vec![
            ("amount", request.amount_cents.to_string()),
            ("currency", request.currency.clone()),
            ("metadata[orderId]", request.order_id.clone()),
        ];
        if let Some(connect) = connect {
            let fee = connect.fee_cents(request.amount_cents)?;
            if fee > 0 {
                fields.push(("application_fee_amount", fee.to_string()));
            }
        }
        let mut http = self
            .http
            .request(Method::POST, &["v1", "payment_intents"])?
            .bearer_auth(self.secret.as_str())
            .form(&fields);
        if let Some(connect) = connect {
            http = http.header("Stripe-Account", &connect.account_id);
        }
        let response = self.http.execute_json(http).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "Stripe",
                "create PaymentIntent",
                response.status,
            ));
        }
        serde_json::from_value(response.body).map_err(|_| shape_error())
    }
    /// # Errors
    /// Returns sanitized provider/network/shape errors.
    pub async fn retrieve_intent(
        &self,
        id: &str,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<PaymentIntent> {
        let mut http = self
            .http
            .request(Method::GET, &["v1", "payment_intents", id])?
            .bearer_auth(self.secret.as_str());
        if let Some(connect) = connect {
            http = http.header("Stripe-Account", &connect.account_id);
        }
        let response = self.http.execute_json(http).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "Stripe",
                "retrieve PaymentIntent",
                response.status,
            ));
        }
        let intent: PaymentIntent =
            serde_json::from_value(response.body).map_err(|_| shape_error())?;
        if intent.id != id {
            return Err(shape_error());
        }
        Ok(intent)
    }
}
#[derive(Debug, Clone)]
pub struct StripeConnect {
    account_id: String,
    fee_bps: u16,
}
impl StripeConnect {
    /// # Errors
    /// Rejects empty/invalid account headers and fees outside 0..=9999 bps.
    pub fn new(account_id: String, fee_bps: u16) -> BlueprintResult<Self> {
        if fee_bps >= 10000
            || account_id.is_empty()
            || !account_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(config_error());
        }
        Ok(Self {
            account_id,
            fee_bps,
        })
    }
    /// Integer equivalent of positive Math.round(amount * bps / 10000), capped
    /// below the charge. Wider arithmetic prevents native monetary overflow.
    /// # Errors
    /// Rejects negative charges.
    pub fn fee_cents(&self, amount: i64) -> BlueprintResult<i64> {
        if amount < 0 {
            return Err(config_error());
        }
        let requested = (i128::from(amount) * i128::from(self.fee_bps) + 5000) / 10000;
        let applied = requested.min(i128::from(amount.saturating_sub(1).max(0)));
        if requested != applied {
            tracing::warn!("Stripe platform fee capped below charge amount");
        }
        i64::try_from(applied).map_err(|_| config_error())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StripePaymentDto {
    #[serde(flatten)]
    pub payment: PaymentDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}
impl std::fmt::Debug for StripePaymentDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripePaymentDto")
            .field("payment", &self.payment)
            .finish_non_exhaustive()
    }
}
impl std::ops::Deref for StripePaymentDto {
    type Target = PaymentDto;
    fn deref(&self) -> &PaymentDto {
        &self.payment
    }
}
impl From<StripePaymentDto> for PaymentDto {
    fn from(value: StripePaymentDto) -> Self {
        value.payment
    }
}
#[derive(Clone)]
pub struct StripePaymentService<S> {
    base: BaseEcommerceService<S>,
    client: StripeClient,
    connect: Option<StripeConnect>,
}
impl<S: EcommerceStore> StripePaymentService<S> {
    #[must_use]
    pub const fn new(base: BaseEcommerceService<S>, client: StripeClient) -> Self {
        Self {
            base,
            client,
            connect: None,
        }
    }
    #[must_use]
    pub fn with_connect(mut self, connect: StripeConnect) -> Self {
        self.connect = Some(connect);
        self
    }
    #[must_use]
    pub const fn base(&self) -> &BaseEcommerceService<S> {
        &self.base
    }
    /// Fetches the existing intent's secret without creating another charge.
    /// # Errors
    /// Returns missing-record/provider errors; provider-less payments are unchanged.
    pub async fn resume_payment(&self, id: &str) -> BlueprintResult<StripePaymentDto> {
        let payment = self.base.get_payment(id).await?;
        let client_secret = if let Some(reference) = &payment.provider_ref {
            self.client
                .retrieve_intent(reference, self.connect.as_ref())
                .await?
                .client_secret
        } else {
            None
        };
        Ok(StripePaymentDto {
            payment,
            client_secret,
        })
    }
}
impl<S: forklaunch_ecommerce_base::payment_resume::PaymentResumeStore> StripePaymentService<S> {
    /// Checks the current local payment/order relation before and after retrieving
    /// a secret. Rejects cancelled/missing orders without provider mutation.
    /// Previously issued secrets and cancellation after the final check remain
    /// outside this local guard; the generic legacy resume method is unchanged.
    /// # Errors
    /// Rejects changed relations, missing/cancelled orders and provider failures.
    pub async fn resume_payment_for_order(&self, id: &str) -> BlueprintResult<StripePaymentDto> {
        let payment = self.base.get_payment(id).await?;
        self.base.store().validate_payment_resume(&payment).await?;
        let client_secret = if let Some(reference) = &payment.provider_ref {
            self.client
                .retrieve_intent(reference, self.connect.as_ref())
                .await?
                .client_secret
        } else {
            None
        };
        self.base.store().validate_payment_resume(&payment).await?;
        Ok(StripePaymentDto {
            payment,
            client_secret,
        })
    }
}
impl<S: EcommerceStore> PaymentService for StripePaymentService<S> {
    type CreatedPayment = StripePaymentDto;
    async fn create_payment(&self, request: CreatePaymentDto) -> BlueprintResult<StripePaymentDto> {
        let intent = self
            .client
            .create_intent(&request, self.connect.as_ref())
            .await?;
        let payment = self
            .base
            .create_payment_with_reference(request, Some(intent.id))
            .await?;
        Ok(StripePaymentDto {
            payment,
            client_secret: intent.client_secret,
        })
    }
    async fn get_payment(&self, id: &str) -> BlueprintResult<PaymentDto> {
        self.base.get_payment(id).await
    }
    async fn confirm_payment(&self, request: ConfirmPaymentDto) -> BlueprintResult<PaymentDto> {
        self.base.confirm_payment(request).await
    }
    async fn fail_payment(&self, request: FailPaymentDto) -> BlueprintResult<PaymentDto> {
        self.base.fail_payment(request).await
    }
}
fn config_error() -> BlueprintError {
    BlueprintError::new(
        "stripe_invalid_config",
        "Invalid Stripe credentials, account or platform fee (expected 0 to 9999 basis points)",
    )
}
fn shape_error() -> BlueprintError {
    BlueprintError::new(
        "stripe_invalid_response",
        "Stripe returned an invalid PaymentIntent",
    )
}
