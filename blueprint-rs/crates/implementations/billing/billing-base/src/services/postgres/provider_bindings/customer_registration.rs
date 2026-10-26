//! Explicit server-authorized billing associations; no external Party FK/proof.
use super::super::{db_error, invalid};
use super::{PostgresBillingStore, StripeBindingScope, conflict};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// The server caller asserts association and existence of this external party.
/// Billing owns only its durable Customer row; provider GET cannot prove that
/// business association. No Serialize/Deserialize or public HTTP ingestion.
pub struct TrustedCustomerParty {
    customer: Uuid,
    party_type: String,
    party_id: String,
}
impl TrustedCustomerParty {
    /// # Errors
    /// Rejects nil local UUIDs and blank/unbounded/control-containing party labels.
    pub fn administrative(
        customer: Uuid,
        party_type: String,
        party_id: String,
    ) -> BlueprintResult<Self> {
        if customer.is_nil()
            || [&party_type, &party_id].iter().any(|v| {
                v.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
                    .is_empty()
                    || v.len() > 256
                    || v.chars().any(char::is_control)
            })
        {
            return Err(invalid());
        }
        Ok(Self {
            customer,
            party_type,
            party_id,
        })
    }
}
/// Sealed local Customer generation; never contains decrypted email/name.
/// Consumed only by the trusted provider adapter after a successful fixed GET.
pub struct CustomerRegistrationSnapshot {
    tenant: String,
    actor: String,
    scope: StripeBindingScope,
    customer: Uuid,
    provider: String,
    party_type: String,
    party_id: String,
    api_version: String,
    row_version: String,
    encrypted_data: Value,
}
/// An immutable billing association generation, not current Stripe evidence or
/// proof that a party still exists in another service. No mutation dispatch API.
#[derive(Clone)]
pub struct DurableStripeCustomerBinding {
    tenant: String,
    scope: StripeBindingScope,
    customer: Uuid,
    generation: Uuid,
    provider: String,
    party_type: String,
    party_id: String,
}
impl DurableStripeCustomerBinding {
    #[must_use]
    pub const fn customer_id(&self) -> Uuid {
        self.customer
    }
    #[must_use]
    pub const fn generation(&self) -> Uuid {
        self.generation
    }
    #[must_use]
    pub fn provider_customer_id(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn party_type(&self) -> &str {
        &self.party_type
    }
    #[must_use]
    pub fn party_id(&self) -> &str {
        &self.party_id
    }
}
type CustomerBindingRow = (String, String, String, String);
fn provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
impl PostgresBillingStore {
    /// Capture only tenant-owned encrypted Customer data and its row generation.
    /// The trusted server supplies the independent party association; email/name
    /// equality, metadata and mutable provider fields are never consulted.
    /// # Errors
    /// Denies missing/mismatched customer records, actor, identifiers or storage.
    pub async fn snapshot_customer_registration(
        &self,
        scope: &StripeBindingScope,
        party: &TrustedCustomerParty,
        provider: &str,
        api_version: &str,
    ) -> BlueprintResult<CustomerRegistrationSnapshot> {
        let actor = self.binding_actor()?.to_owned();
        if !provider_id(provider)
            || api_version.is_empty()
            || api_version.len() > 128
            || api_version.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let mut tx = self.transaction().await?;
        let row:Option<(String,sqlx::types::Json<Value>)>=sqlx::query_as("SELECT xmin::text,data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='customer' AND id=$2")
            .bind(self.tenant.id()).bind(party.customer.to_string()).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let (row_version, data) = row.ok_or_else(conflict)?;
        if data.0["id"] != party.customer.to_string() {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(CustomerRegistrationSnapshot {
            tenant: self.tenant.id().into(),
            actor,
            scope: scope.clone(),
            customer: party.customer,
            provider: provider.into(),
            party_type: party.party_type.clone(),
            party_id: party.party_id.clone(),
            api_version: api_version.into(),
            row_version,
            encrypted_data: data.0,
        })
    }
    /// Trusted adapter only: caller already authenticated exact Customer evidence.
    /// Locks local Customer before the binding, rechecks the complete encrypted
    /// snapshot and fresh provider context after every awaited publication stage.
    /// A COMMIT error requires scoped inspection; it does not establish rollback.
    /// # Errors
    /// Denies changed rows/associations/actors, expired evidence or failed audits.
    pub async fn complete_customer_registration<F>(
        &self,
        snapshot: CustomerRegistrationSnapshot,
        fresh: F,
    ) -> BlueprintResult<DurableStripeCustomerBinding>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        if snapshot.tenant != self.tenant.id() || snapshot.actor != self.binding_actor()? {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let row:Option<(String,sqlx::types::Json<Value>)>=sqlx::query_as("SELECT xmin::text,data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='customer' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(snapshot.customer.to_string()).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let (version, data) = row.ok_or_else(conflict)?;
        if version != snapshot.row_version
            || data.0 != snapshot.encrypted_data
            || data.0["id"] != snapshot.customer.to_string()
        {
            return Err(conflict());
        }
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        let mut binding = DurableStripeCustomerBinding {
            tenant: snapshot.tenant,
            scope: snapshot.scope,
            customer: snapshot.customer,
            generation: Uuid::new_v4(),
            provider: snapshot.provider,
            party_type: snapshot.party_type,
            party_id: snapshot.party_id,
        };
        let inserted=sqlx::query("INSERT INTO fl_billing.stripe_customer_bindings(tenant_id,deployment_label,account_id,livemode,local_customer_id,binding_id,storage_id,provider_customer_id,party_type,party_id,verified_api_version) VALUES($1,$2,$3,$4,$5::uuid,$6::uuid,$5,$7,$8,$9,$10) ON CONFLICT(tenant_id,deployment_label,account_id,livemode,local_customer_id) DO NOTHING")
            .bind(self.tenant.id()).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(binding.customer.to_string()).bind(binding.generation.to_string()).bind(&binding.provider).bind(&binding.party_type).bind(&binding.party_id).bind(snapshot.api_version).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if inserted == 0 {
            let existing = self
                .customer_binding_row(&mut tx, &binding.scope, binding.customer, true)
                .await?
                .ok_or_else(conflict)?;
            if existing.1 != binding.provider
                || existing.2 != binding.party_type
                || existing.3 != binding.party_id
            {
                return Err(conflict());
            }
            binding.generation = Uuid::parse_str(&existing.0).map_err(|_| invalid())?;
        }
        self.audit_customer_binding(
            &mut tx,
            &binding,
            if inserted == 1 { "register" } else { "verify" },
        )
        .await?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(db_error)?;
        Ok(binding)
    }
    async fn customer_binding_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: &StripeBindingScope,
        customer: Uuid,
        lock: bool,
    ) -> BlueprintResult<Option<CustomerBindingRow>> {
        let suffix = if lock { " FOR UPDATE OF b" } else { "" };
        sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT b.binding_id::text,b.provider_customer_id,b.party_type,b.party_id FROM fl_billing.stripe_customer_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id WHERE b.tenant_id=$1 AND b.deployment_label=$2 AND b.account_id=$3 AND b.livemode=$4 AND b.local_customer_id=$5::uuid AND r.data->>'id'=b.storage_id{suffix}")))
            .bind(self.tenant.id()).bind(&scope.deployment_label).bind(&scope.account_id).bind(scope.livemode).bind(customer.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)
    }
    /// Historical tenant-scoped association lookup; never current provider proof.
    /// # Errors
    /// Denies missing actor/nil IDs or storage/malformed generation failures.
    pub async fn stripe_customer_binding(
        &self,
        scope: &StripeBindingScope,
        customer: Uuid,
    ) -> BlueprintResult<Option<DurableStripeCustomerBinding>> {
        self.binding_actor()?;
        if customer.is_nil() {
            return Err(invalid());
        }
        let mut tx = self.transaction().await?;
        let row = self
            .customer_binding_row(&mut tx, scope, customer, false)
            .await?;
        tx.commit().await.map_err(db_error)?;
        row.map(|(generation, provider, party_type, party_id)| {
            Ok(DurableStripeCustomerBinding {
                tenant: self.tenant.id().into(),
                scope: scope.clone(),
                customer,
                generation: Uuid::parse_str(&generation).map_err(|_| invalid())?,
                provider,
                party_type,
                party_id,
            })
        })
        .transpose()
    }
    /// CAS revoke the exact local association generation. No provider effects.
    /// This API does not coordinate deletion of a party in an external service.
    /// Future Subscription dispatch requires a separate persistent claims barrier.
    /// # Errors
    /// Denies foreign/stale associations and rolls back on audit failure.
    pub async fn revoke_stripe_customer_binding(
        &self,
        binding: &DurableStripeCustomerBinding,
    ) -> BlueprintResult<()> {
        self.binding_actor()?;
        if binding.tenant != self.tenant.id() {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let changed=sqlx::query("DELETE FROM fl_billing.stripe_customer_bindings WHERE tenant_id=$1 AND deployment_label=$2 AND account_id=$3 AND livemode=$4 AND local_customer_id=$5::uuid AND binding_id=$6::uuid AND provider_customer_id=$7 AND party_type=$8 AND party_id=$9")
            .bind(self.tenant.id()).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(binding.customer.to_string()).bind(binding.generation.to_string()).bind(&binding.provider).bind(&binding.party_type).bind(&binding.party_id).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        self.audit_customer_binding(&mut tx, binding, "revoke")
            .await?;
        tx.commit().await.map_err(db_error)
    }
    async fn audit_customer_binding(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &DurableStripeCustomerBinding,
        operation: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_billing.stripe_customer_binding_audit(id,tenant_id,binding_id,local_customer_id,deployment_label,account_id,livemode,provider_customer_id,party_type,party_id,actor_id,correlation_id,operation) VALUES($1::uuid,$2,$3::uuid,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(binding.generation.to_string()).bind(binding.customer.to_string()).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(&binding.provider).bind(&binding.party_type).bind(&binding.party_id).bind(self.binding_actor()?).bind(&self.audit.correlation_id).bind(operation).execute(&mut **tx).await.map_err(db_error)?;
        Ok(())
    }
}
