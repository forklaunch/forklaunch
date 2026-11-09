//! Explicit administrative identity provisioning. No provider verification,
//! inferred key context or automatic backfill. Provider wrappers must separately
//! authenticate scope before resolving and reading a binding.
mod portal_creation;
use super::{PostgresBillingStore, db_error, invalid, validate_id};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
pub use portal_creation::{
    PortalCreationClaim, PortalCreationPending, PortalCreationReservation, PortalCreationStatus,
    PortalDependencySnapshot, TrustedPortalPublisher,
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
mod customer_registration;
pub use customer_registration::{
    CustomerRegistrationSnapshot, DurableStripeCustomerBinding, TrustedCustomerParty,
};
mod paused_effect;
mod payment_link_creation;
mod plan_creation;
mod subscription_creation;
pub use paused_effect::{PausedEffectAcknowledgement, PausedSubscriptionEffectSnapshot};
pub use payment_link_creation::{
    ManagedPaymentLinkPublication, PaymentLinkCreationClaim, PaymentLinkCreationReadSnapshot,
    PaymentLinkCreationReservation, PaymentLinkCreationStatus, PaymentLinkExpirationClaim,
    PaymentLinkExpirationRecovery, PaymentLinkExpirationReservation, PaymentLinkExpirationStatus,
    TrustedPaymentLinkExpirationPublisher,
};
pub use plan_creation::{
    PlanCreationClaim, PlanCreationReadSnapshot, PlanCreationReservation, PlanCreationStatus,
};
pub use subscription_creation::{
    SubscriptionCreationClaim, SubscriptionCreationPending, SubscriptionCreationReadSnapshot,
    SubscriptionCreationRecovery, SubscriptionCreationReservation, SubscriptionCreationStatus,
    SubscriptionDependencySnapshot, TrustedSubscriptionPublisher,
};

type BindingRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Trusted server configuration, never an HTTP DTO or a claim extracted from
/// providerFields. Construction does not authenticate this scope; administrators
/// provision it explicitly against their configured account, and provider wrappers
/// must independently verify the configured account and mode.
#[derive(Clone, PartialEq, Eq)]
pub struct StripeBindingScope {
    deployment_label: String,
    account_id: String,
    livemode: bool,
}
impl StripeBindingScope {
    #[must_use]
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    #[must_use]
    pub const fn livemode(&self) -> bool {
        self.livemode
    }
    /// # Errors
    /// Rejects empty/unsafe labels and non-account identifiers. This validates
    /// syntax only; it does not prove control of an account or API key mode.
    pub fn administrative(
        deployment_label: String,
        account_id: String,
        livemode: bool,
    ) -> BlueprintResult<Self> {
        for value in [&deployment_label, &account_id] {
            if value.is_empty()
                || value.len() > 128
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            {
                return Err(invalid());
            }
        }
        if !account_id.starts_with("acct_") || account_id.len() <= 5 {
            return Err(invalid());
        }
        Ok(Self {
            deployment_label,
            account_id,
            livemode,
        })
    }
}
/// Explicit administrative assertion linking a provider customer to a local party.
/// This is NOT authenticated provider evidence and never reads providerFields.
#[derive(Clone, PartialEq, Eq)]
pub struct StripeSubscriptionOwnership {
    provider_customer_id: String,
    local_party_id: String,
    local_party_type: String,
}
impl StripeSubscriptionOwnership {
    /// # Errors
    /// Rejects malformed customer IDs and empty/unsafe local party identifiers.
    pub fn administrative(
        provider_customer_id: String,
        local_party_id: String,
        local_party_type: String,
    ) -> BlueprintResult<Self> {
        if !provider_customer_id.starts_with("cus_")
            || provider_customer_id.len() <= 4
            || provider_customer_id.len() > 128
            || !provider_customer_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(invalid());
        }
        for value in [&local_party_id, &local_party_type] {
            validate_id(value)?;
            if value.trim().is_empty() {
                return Err(invalid());
            }
        }
        Ok(Self {
            provider_customer_id,
            local_party_id,
            local_party_type,
        })
    }
    #[must_use]
    pub fn provider_customer_id(&self) -> &str {
        &self.provider_customer_id
    }
    #[must_use]
    pub fn local_party_id(&self) -> &str {
        &self.local_party_id
    }
    #[must_use]
    pub fn local_party_type(&self) -> &str {
        &self.local_party_type
    }
    fn matches(&self, row: &serde_json::Value, provider_id: &str) -> bool {
        row["externalId"] == provider_id
            && row["partyId"] == self.local_party_id
            && row["partyType"] == self.local_party_type
    }
}

/// Opaque generation-bound administrative alias. Storage keys and encrypted
/// envelopes remain unchanged. No Serialize/Deserialize or public constructor.
#[derive(Clone)]
pub struct DurableStripeBinding {
    tenant: String,
    scope: StripeBindingScope,
    kind: BillingKind,
    public_id: Uuid,
    binding_id: Uuid,
    storage_id: String,
    provider_id: String,
    subscription_ownership: Option<StripeSubscriptionOwnership>,
}
impl DurableStripeBinding {
    #[must_use]
    pub fn subscription_ownership(&self) -> Option<&StripeSubscriptionOwnership> {
        self.subscription_ownership.as_ref()
    }

    #[must_use]
    pub const fn public_id(&self) -> Uuid {
        self.public_id
    }
    #[must_use]
    pub fn storage_id(&self) -> &str {
        &self.storage_id
    }
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }
}
fn binding_kind(kind: BillingKind) -> BlueprintResult<()> {
    if !matches!(
        kind,
        BillingKind::Plan
            | BillingKind::CheckoutSession
            | BillingKind::PaymentLink
            | BillingKind::BillingPortal
            | BillingKind::Subscription
    ) {
        return Err(invalid());
    }
    Ok(())
}
fn conflict() -> BlueprintError {
    BlueprintError::new(
        "billing_provider_binding_conflict",
        "Billing provider binding or ownership does not match",
    )
}
impl PostgresBillingStore {
    fn binding_actor(&self) -> BlueprintResult<&str> {
        let actor = self.audit.actor_id.as_deref().ok_or_else(invalid)?;
        validate_id(actor)?;
        if actor.trim().is_empty() {
            return Err(invalid());
        }
        Ok(actor)
    }
    /// Reads and audits durable ownership joined to exactly this binding generation.
    /// Captures binding and record in one statement snapshot. Does not
    /// authenticate a provider context; callers must verify it around network/DB work.
    /// # Errors
    /// Rejects missing actors, foreign tenants, revoked/replaced aliases, invalid
    /// ciphertext and audit failures. Cache-only ownership is never read here.
    pub async fn read_stripe_bound_record(
        &self,
        binding: &DurableStripeBinding,
    ) -> BlueprintResult<serde_json::Value> {
        self.binding_actor()?;
        if binding.tenant != self.tenant.id() {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let row:Option<sqlx::types::Json<serde_json::Value>> = sqlx::query_scalar("SELECT r.data FROM fl_billing.provider_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id WHERE b.tenant_id=$1 AND b.kind=$2 AND b.public_id=$3::uuid AND b.binding_id=$4::uuid AND b.storage_id=$5 AND b.provider_id=$6 AND b.deployment_label=$7 AND b.account_id=$8 AND b.livemode=$9 AND b.subscription_provider_customer_id IS NOT DISTINCT FROM $10 AND b.local_party_id IS NOT DISTINCT FROM $11 AND b.local_party_type IS NOT DISTINCT FROM $12")
            .bind(self.tenant.id()).bind(binding.kind.as_str()).bind(binding.public_id.to_string()).bind(binding.binding_id.to_string())
            .bind(&binding.storage_id).bind(&binding.provider_id).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode)
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.provider_customer_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_type))
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let value = self
            .decode(
                &mut tx,
                Scope::Durable,
                binding.kind,
                &binding.storage_id,
                row.ok_or_else(conflict)?.0,
                "provider_binding_read",
            )
            .await?;
        if binding
            .subscription_ownership
            .as_ref()
            .is_some_and(|relation| !relation.matches(&value, &binding.provider_id))
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(value)
    }
    /// Trusted administrative provisioning ONLY, not a provider-evidence verifier.
    /// The caller explicitly asserts the provider/account/mode association. Requires
    /// an existing durable owned record; rejects cache-only records. Public IDs,
    /// storage IDs and provider IDs cannot be reassigned while a binding exists.
    /// # Errors
    /// Rejects invalid identifiers, absent actor/ownership, duplicate bindings and
    /// audit failures. All binding/audit writes commit together.
    pub async fn provision_stripe_binding(
        &self,
        scope: &StripeBindingScope,
        kind: BillingKind,
        public_id: Uuid,
        storage_id: &str,
        provider_id: &str,
    ) -> BlueprintResult<DurableStripeBinding> {
        if kind == BillingKind::Subscription {
            return Err(invalid());
        }
        self.provision_binding(scope, kind, public_id, storage_id, provider_id, None)
            .await
    }
    /// Administrative subscription provisioning; no HTTP ownership proof.
    /// Later provider verification must compare the exact asserted customer.
    /// # Errors
    /// Requires an existing durable row matching externalId and local party relation;
    /// rejects missing actors, conflicting aliases and audit failures atomically.
    pub async fn provision_stripe_subscription_binding(
        &self,
        scope: &StripeBindingScope,
        public_id: Uuid,
        storage_id: &str,
        provider_id: &str,
        ownership: &StripeSubscriptionOwnership,
    ) -> BlueprintResult<DurableStripeBinding> {
        if !provider_id.starts_with("sub_")
            || provider_id.len() <= 4
            || provider_id.len() > 128
            || !provider_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(invalid());
        }
        self.provision_binding(
            scope,
            BillingKind::Subscription,
            public_id,
            storage_id,
            provider_id,
            Some(ownership.clone()),
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn provision_binding(
        &self,
        scope: &StripeBindingScope,
        kind: BillingKind,
        public_id: Uuid,
        storage_id: &str,
        provider_id: &str,
        ownership: Option<StripeSubscriptionOwnership>,
    ) -> BlueprintResult<DurableStripeBinding> {
        binding_kind(kind)?;
        validate_id(storage_id)?;
        validate_id(provider_id)?;
        if public_id.is_nil() {
            return Err(invalid());
        }
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        let owned: Option<sqlx::types::Json<serde_json::Value>> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind=$2 AND id=$3 FOR UPDATE")
            .bind(self.tenant.id()).bind(kind.as_str()).bind(storage_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let owned = owned.ok_or_else(conflict)?;
        if let Some(relation) = &ownership {
            let row = self
                .decode(
                    &mut tx,
                    Scope::Durable,
                    kind,
                    storage_id,
                    owned.0,
                    "provider_binding_provision",
                )
                .await?;
            if !relation.matches(&row, provider_id) {
                return Err(conflict());
            }
        }
        let binding = DurableStripeBinding {
            tenant: self.tenant.id().into(),
            scope: scope.clone(),
            kind,
            public_id,
            binding_id: Uuid::new_v4(),
            storage_id: storage_id.into(),
            provider_id: provider_id.into(),
            subscription_ownership: ownership,
        };
        sqlx::query("INSERT INTO fl_billing.provider_bindings(tenant_id,kind,public_id,binding_id,storage_id,deployment_label,account_id,livemode,provider_id,subscription_provider_customer_id,local_party_id,local_party_type) VALUES($1,$2,$3::uuid,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(self.tenant.id()).bind(kind.as_str()).bind(public_id.to_string()).bind(binding.binding_id.to_string())
            .bind(storage_id).bind(&scope.deployment_label).bind(&scope.account_id).bind(scope.livemode).bind(provider_id)
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.provider_customer_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_type))
            .execute(&mut *tx).await.map_err(db_error)?;
        self.audit_binding(&mut tx, &binding, "provision").await?;
        tx.commit().await.map_err(db_error)?;
        Ok(binding)
    }
    /// Looks up an explicitly provisioned durable alias in exactly this scope.
    /// This does not authenticate a provider account or enable runtime routing.
    /// # Errors
    /// Rejects invalid kind/ID and returns sanitized storage failures.
    pub async fn stripe_binding_by_public_id(
        &self,
        scope: &StripeBindingScope,
        kind: BillingKind,
        public_id: Uuid,
    ) -> BlueprintResult<Option<DurableStripeBinding>> {
        self.find_stripe_binding(scope, kind, &public_id.to_string(), false)
            .await
    }
    /// Reverse lookup for a trusted provider context, not a public DTO claim.
    /// # Errors
    /// Rejects invalid kind/ID and returns sanitized storage failures.
    pub async fn stripe_binding_by_provider_id(
        &self,
        scope: &StripeBindingScope,
        kind: BillingKind,
        provider_id: &str,
    ) -> BlueprintResult<Option<DurableStripeBinding>> {
        self.find_stripe_binding(scope, kind, provider_id, true)
            .await
    }
    async fn find_stripe_binding(
        &self,
        scope: &StripeBindingScope,
        kind: BillingKind,
        id: &str,
        provider: bool,
    ) -> BlueprintResult<Option<DurableStripeBinding>> {
        binding_kind(kind)?;
        validate_id(id)?;
        let column = if provider {
            "provider_id"
        } else {
            "public_id::text"
        };
        let mut tx = self.transaction().await?;
        let row:Option<BindingRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT binding_id::text,public_id::text,storage_id,provider_id,subscription_provider_customer_id,local_party_id,local_party_type FROM fl_billing.provider_bindings WHERE tenant_id=$1 AND deployment_label=$2 AND account_id=$3 AND livemode=$4 AND kind=$5 AND {column}=$6")))
            .bind(self.tenant.id()).bind(&scope.deployment_label).bind(&scope.account_id).bind(scope.livemode).bind(kind.as_str()).bind(id)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        row.map(
            |(binding_id, public_id, storage_id, provider_id, customer, party, party_type)| {
                let subscription_ownership = match (customer, party, party_type) {
                    (Some(customer), Some(party), Some(party_type))
                        if kind == BillingKind::Subscription =>
                    {
                        Some(StripeSubscriptionOwnership::administrative(
                            customer, party, party_type,
                        )?)
                    }
                    (None, None, None) if kind != BillingKind::Subscription => None,
                    _ => return Err(conflict()),
                };
                Ok(DurableStripeBinding {
                    tenant: self.tenant.id().into(),
                    scope: scope.clone(),
                    kind,
                    public_id: Uuid::parse_str(&public_id).map_err(|_| invalid())?,
                    binding_id: Uuid::parse_str(&binding_id).map_err(|_| invalid())?,
                    storage_id,
                    provider_id,
                    subscription_ownership,
                })
            },
        )
        .transpose()
    }
    /// Revokes precisely this generation; does not delete the owned record or
    /// mutate Stripe. Revoke before deleting a durable record with an alias.
    /// # Errors
    /// Rejects absent actors, foreign tenants, stale generations and audit failure.
    pub async fn revoke_stripe_binding(
        &self,
        binding: &DurableStripeBinding,
    ) -> BlueprintResult<()> {
        self.binding_actor()?;
        if binding.tenant != self.tenant.id() {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let changed = sqlx::query("DELETE FROM fl_billing.provider_bindings WHERE tenant_id=$1 AND kind=$2 AND public_id=$3::uuid AND binding_id=$4::uuid AND subscription_provider_customer_id IS NOT DISTINCT FROM $5 AND local_party_id IS NOT DISTINCT FROM $6 AND local_party_type IS NOT DISTINCT FROM $7")
            .bind(self.tenant.id()).bind(binding.kind.as_str()).bind(binding.public_id.to_string()).bind(binding.binding_id.to_string())
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.provider_customer_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_type))
            .execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        self.audit_binding(&mut tx, binding, "revoke").await?;
        tx.commit().await.map_err(db_error)
    }
    async fn audit_binding(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &DurableStripeBinding,
        operation: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_billing.provider_binding_audit(id,tenant_id,binding_id,public_id,kind,storage_id,deployment_label,account_id,livemode,provider_id,operation,actor_id,correlation_id,subscription_provider_customer_id,local_party_id,local_party_type) VALUES($1::uuid,$2,$3::uuid,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(binding.binding_id.to_string()).bind(binding.public_id.to_string())
            .bind(binding.kind.as_str()).bind(&binding.storage_id).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id)
            .bind(binding.scope.livemode).bind(&binding.provider_id).bind(operation).bind(self.binding_actor()?)
            .bind(&self.audit.correlation_id)
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.provider_customer_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_id))
            .bind(binding.subscription_ownership.as_ref().map(|v| &v.local_party_type))
            .execute(&mut **tx).await.map_err(db_error)?;
        Ok(())
    }
}

mod checkout_creation;
pub use checkout_creation::{
    CheckoutCreationClaim, CheckoutCreationPending, CheckoutCreationReadSnapshot,
    CheckoutCreationReservation, CheckoutCreationStatus, CheckoutDependencySnapshot,
    TrustedCheckoutPublisher,
};
