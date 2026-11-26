//! Durable administrative scope assertions; never provider ownership proof.
mod paypal_cancellation;
mod scoped_cancellation;
pub use paypal_cancellation::{
    CancelledPaypalOrderPaymentSnapshot, ScopedPaypalCancellationOutcome,
    ScopedPaypalCancellationReceipt,
};
mod scoped_capture;
use super::{PostgresEcommerceStore, storage_error};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::PaymentDto;
pub use scoped_cancellation::{
    CancelledOrderPaymentSnapshot, ScopedCancellationOutcome, ScopedCancellationReceipt,
};
pub use scoped_capture::{CapturedPaymentSnapshot, ScopedCaptureOutcome, ScopedCaptureReceipt};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Trusted server configuration. Constructing this does not authenticate a merchant.
#[derive(Clone, PartialEq, Eq)]
pub struct PaymentProviderScope {
    provider: String,
    deployment_label: String,
    merchant_id: String,
    livemode: bool,
}
impl PaymentProviderScope {
    /// Explicit administrator assertion, never derived from provider fields or keys.
    /// # Errors
    /// Rejects unsupported providers and unsafe or blank identifiers.
    pub fn administrative(
        provider: String,
        deployment_label: String,
        merchant_id: String,
        livemode: bool,
    ) -> BlueprintResult<Self> {
        if !matches!(provider.as_str(), "stripe" | "paypal")
            || !label(&deployment_label)
            || !label(&merchant_id)
            || (provider == "stripe"
                && (!merchant_id.starts_with("acct_") || merchant_id.len() <= 5))
        {
            return Err(invalid());
        }
        Ok(Self {
            provider,
            deployment_label,
            merchant_id,
            livemode,
        })
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }
    #[must_use]
    pub const fn livemode(&self) -> bool {
        self.livemode
    }
}
/// Opaque tenant-bound generation and immutable payment relation. Not a mutation
/// capability or authenticated provider evidence; no HTTP deserialization exists.
#[derive(Clone, PartialEq, Eq)]
pub struct PaymentProviderBinding {
    tenant_id: String,
    payment_id: String,
    scope: PaymentProviderScope,
    generation: String,
    provider_ref: String,
    order_id: String,
    amount_cents: i64,
    currency: String,
}
impl PaymentProviderBinding {
    #[must_use]
    pub fn payment_id(&self) -> &str {
        &self.payment_id
    }
    #[must_use]
    pub fn provider_ref(&self) -> &str {
        &self.provider_ref
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub fn scope(&self) -> &PaymentProviderScope {
        &self.scope
    }
    #[must_use]
    pub fn order_id(&self) -> &str {
        &self.order_id
    }
    #[must_use]
    pub const fn amount_cents(&self) -> i64 {
        self.amount_cents
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
}
fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_invalid_provider_binding",
        "Invalid administrative payment provider binding",
    )
}
fn conflict() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_provider_binding_conflict",
        "Payment provider binding is unavailable or changed",
    )
}
fn reference(scope: &PaymentProviderScope, value: &str) -> bool {
    label(value) && (scope.provider != "stripe" || (value.starts_with("pi_") && value.len() > 3))
}
impl PostgresEcommerceStore {
    fn binding_actor(&self) -> BlueprintResult<()> {
        if self
            .audit
            .actor_id
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
            || self.audit.correlation_id.trim().is_empty()
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Explicit provisioning against an exact locked durable payment snapshot.
    /// Retains storage keys, ciphertext and provider references. No provider call.
    /// # Errors
    /// Rejects invalid actors, stale snapshots, missing orders and existing bindings.
    pub async fn provision_payment_provider_binding(
        &self,
        scope: &PaymentProviderScope,
        expected: &PaymentDto,
    ) -> BlueprintResult<PaymentProviderBinding> {
        self.binding_actor()?;
        let provider_ref = expected
            .provider_ref
            .as_deref()
            .filter(|v| reference(scope, v))
            .ok_or_else(invalid)?;
        if !label(&expected.id)
            || !label(&expected.order_id)
            || expected.amount_cents <= 0
            || expected.currency.len() != 3
            || !expected.currency.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return Err(invalid());
        }
        let mut tx = self.transaction().await?;
        let stored: Option<sqlx::types::Json<PaymentDto>> = sqlx::query_scalar("SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(&expected.id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        if stored.as_ref().map(|v| &v.0) != Some(expected) {
            return Err(conflict());
        }
        let order: Option<String> = sqlx::query_scalar("SELECT id FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND id=$2 AND data->>'id'=id FOR KEY SHARE")
            .bind(self.tenant.id()).bind(&expected.order_id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        if order.is_none() {
            return Err(conflict());
        }
        let binding = PaymentProviderBinding {
            tenant_id: self.tenant.id().into(),
            payment_id: expected.id.clone(),
            scope: scope.clone(),
            generation: Uuid::new_v4().to_string(),
            provider_ref: provider_ref.into(),
            order_id: expected.order_id.clone(),
            amount_cents: expected.amount_cents,
            currency: expected.currency.to_ascii_uppercase(),
        };
        let inserted=sqlx::query("INSERT INTO fl_ecommerce.payment_provider_bindings(tenant_id,payment_id,provider,deployment_label,merchant_id,livemode,generation,provider_ref,order_id,amount_cents,currency) VALUES($1,$2,$3,$4,$5,$6,$7::uuid,$8,$9,$10,$11) ON CONFLICT DO NOTHING")
            .bind(&binding.tenant_id).bind(&binding.payment_id).bind(&scope.provider).bind(&scope.deployment_label).bind(&scope.merchant_id).bind(scope.livemode).bind(&binding.generation).bind(&binding.provider_ref).bind(&binding.order_id).bind(binding.amount_cents).bind(&binding.currency)
            .execute(&mut *tx).await.map_err(storage_error)?.rows_affected();
        if inserted != 1 {
            return Err(conflict());
        }
        self.binding_audit(&mut tx, &binding, "provision").await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(binding)
    }
    /// Reads only a matching live relation; status/time changes are allowed.
    /// A later mutation must independently recheck generation, scope and relation.
    /// # Errors
    /// Rejects missing/foreign/stale bindings or changed payment/order relations.
    pub async fn payment_provider_binding(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
    ) -> BlueprintResult<PaymentProviderBinding> {
        if !label(payment_id) {
            return Err(invalid());
        }
        let mut tx = self.transaction().await?;
        let row:Option<(String,String,String,i64,String)>=sqlx::query_as("SELECT b.generation::text,b.provider_ref,b.order_id,b.amount_cents,b.currency FROM fl_ecommerce.payment_provider_bindings b JOIN fl_ecommerce.records p ON p.tenant_id=b.tenant_id AND p.kind='payment' AND p.id=b.payment_id JOIN fl_ecommerce.records o ON o.tenant_id=b.tenant_id AND o.kind='order' AND o.id=b.order_id WHERE b.tenant_id=$1 AND b.payment_id=$2 AND b.provider=$3 AND b.deployment_label=$4 AND b.merchant_id=$5 AND b.livemode=$6 AND p.data->>'id'=b.payment_id AND o.data->>'id'=b.order_id AND p.data->>'orderId'=b.order_id AND p.data->>'providerRef'=b.provider_ref AND p.data->>'amountCents'=b.amount_cents::text AND upper(p.data->>'currency')=b.currency")
            .bind(self.tenant.id()).bind(payment_id).bind(&scope.provider).bind(&scope.deployment_label).bind(&scope.merchant_id).bind(scope.livemode).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let (generation, provider_ref, order_id, amount_cents, currency) =
            row.ok_or_else(conflict)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(PaymentProviderBinding {
            tenant_id: self.tenant.id().into(),
            payment_id: payment_id.into(),
            scope: scope.clone(),
            generation,
            provider_ref,
            order_id,
            amount_cents,
            currency,
        })
    }
    /// Revokes only the exact tenant-bound generation; keeps the audit history.
    /// # Errors
    /// Rejects missing actors, other tenants and stale/replaced generations.
    pub async fn revoke_payment_provider_binding(
        &self,
        binding: &PaymentProviderBinding,
    ) -> BlueprintResult<()> {
        self.binding_actor()?;
        if binding.tenant_id != self.tenant.id() {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let removed=sqlx::query("DELETE FROM fl_ecommerce.payment_provider_bindings WHERE tenant_id=$1 AND payment_id=$2 AND generation=$3::uuid AND provider=$4 AND deployment_label=$5 AND merchant_id=$6 AND livemode=$7 AND provider_ref=$8 AND order_id=$9 AND amount_cents=$10 AND currency=$11")
            .bind(self.tenant.id()).bind(&binding.payment_id).bind(&binding.generation).bind(&binding.scope.provider).bind(&binding.scope.deployment_label).bind(&binding.scope.merchant_id).bind(binding.scope.livemode).bind(&binding.provider_ref).bind(&binding.order_id).bind(binding.amount_cents).bind(&binding.currency).execute(&mut *tx).await.map_err(storage_error)?.rows_affected();
        if removed != 1 {
            return Err(conflict());
        }
        self.binding_audit(&mut tx, binding, "revoke").await?;
        tx.commit().await.map_err(storage_error)
    }
    async fn binding_audit(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &PaymentProviderBinding,
        operation: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_ecommerce.payment_provider_binding_audit(id,tenant_id,payment_id,generation,operation,provider,deployment_label,merchant_id,livemode,provider_ref,order_id,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)")
            .bind(Uuid::new_v4().to_string()).bind(&binding.tenant_id).bind(&binding.payment_id).bind(&binding.generation).bind(operation).bind(&binding.scope.provider).bind(&binding.scope.deployment_label).bind(&binding.scope.merchant_id).bind(binding.scope.livemode).bind(&binding.provider_ref).bind(&binding.order_id).bind(binding.amount_cents).bind(&binding.currency).bind(&self.audit.actor_id).bind(&self.audit.correlation_id).execute(&mut **tx).await.map_err(storage_error)?;
        Ok(())
    }
}
