//! Stripe Tax with an explicitly marked seven-percent fallback on provider failure.
use crate::StripeClient;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use forklaunch_ecommerce_interface::{
    TaxLineDto,
    pricing::{TaxCalculation, TaxCalculationResult, TaxService},
};
use reqwest::Method;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct StripeTaxService {
    stripe: StripeClient,
}
impl StripeTaxService {
    #[must_use]
    pub fn new(stripe: StripeClient) -> Self {
        Self { stripe }
    }
    async fn calculate_provider(
        &self,
        request: &TaxCalculation,
    ) -> BlueprintResult<TaxCalculationResult> {
        let mut fields = vec![
            ("currency".to_owned(), "usd".to_owned()),
            (
                "customer_details[address_source]".to_owned(),
                "shipping".to_owned(),
            ),
        ];
        for (index, amount) in request.line_item_cents.iter().enumerate() {
            for (key, value) in [
                ("amount", amount.to_string()),
                ("reference", format!("line-{index}")),
                ("tax_behavior", "exclusive".to_owned()),
            ] {
                fields.push((format!("line_items[{index}][{key}]"), value));
            }
        }
        let address = &request.shipping_address;
        for (key, value) in [
            ("line1", &address.line1),
            ("city", &address.city),
            ("state", &address.state),
            ("postal_code", &address.postal_code),
            ("country", &address.country),
        ] {
            fields.push((format!("customer_details[address][{key}]"), value.clone()));
        }
        if let Some(line2) = &address.line2 {
            fields.push(("customer_details[address][line2]".to_owned(), line2.clone()));
        }
        let http = self
            .stripe
            .http
            .request(Method::POST, &["v1", "tax", "calculations"])?
            .bearer_auth(self.stripe.secret.as_str())
            .form(&fields);
        let response = self.stripe.http.execute_json(http).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "Stripe",
                "calculate tax",
                response.status,
            ));
        }
        let response: Calculation =
            serde_json::from_value(response.body).map_err(|_| crate::shape_error())?;
        if response.tax_amount_exclusive < 0
            || response.tax_breakdown.iter().any(|line| line.amount < 0)
        {
            return Err(crate::shape_error());
        }
        Ok(TaxCalculationResult {
            tax_cents: response.tax_amount_exclusive,
            estimated: false,
            breakdown: response
                .tax_breakdown
                .into_iter()
                .map(|line| {
                    let jurisdiction = [line.tax_rate_details.state, line.tax_rate_details.country]
                        .into_iter()
                        .flatten()
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join(", ");
                    TaxLineDto {
                        jurisdiction: if jurisdiction.is_empty() {
                            "unknown".to_owned()
                        } else {
                            jurisdiction
                        },
                        tax_cents: line.amount,
                    }
                })
                .collect(),
        })
    }
}
impl TaxService for StripeTaxService {
    async fn calculate(&self, request: TaxCalculation) -> BlueprintResult<TaxCalculationResult> {
        let subtotal = request
            .line_item_cents
            .iter()
            .try_fold(0_i64, |total, amount| {
                if *amount < 0 {
                    None
                } else {
                    total.checked_add(*amount)
                }
            })
            .ok_or_else(|| {
                BlueprintError::new(
                    "ecommerce_validation",
                    "Tax amounts must be nonnegative and fit integral cents",
                )
            })?;
        if let Ok(result) = self.calculate_provider(&request).await {
            Ok(result)
        } else {
            // Avoid floats and overflow; positive half cents round upward like Math.round.
            let tax_cents = i64::try_from((i128::from(subtotal) * 7 + 50) / 100).map_err(|_| {
                BlueprintError::new("ecommerce_validation", "Estimated tax overflow")
            })?;
            tracing::warn!("Stripe Tax unavailable; using explicitly estimated flat-rate tax");
            Ok(TaxCalculationResult {
                tax_cents,
                estimated: true,
                breakdown: vec![TaxLineDto {
                    jurisdiction: "estimated-fallback".to_owned(),
                    tax_cents,
                }],
            })
        }
    }
}
#[derive(Deserialize)]
struct Calculation {
    tax_amount_exclusive: i64,
    #[serde(default)]
    tax_breakdown: Vec<TaxLine>,
}
#[derive(Deserialize)]
struct TaxLine {
    amount: i64,
    tax_rate_details: TaxRateDetails,
}
#[derive(Deserialize)]
struct TaxRateDetails {
    state: Option<String>,
    country: Option<String>,
}
