//! Completed creation provenance for public-UUID Checkout reads, never dispatch authority.
use super::super::DurableStripeBinding;
use super::{PostgresBillingStore, conflict, db_error, storage_error};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use uuid::Uuid;

/// Opaque read snapshot. Contains neither a dispatch key nor publication authority.
/// Current Customer registration and local body must still match creation evidence.
pub struct CheckoutCreationReadSnapshot {
    binding: DurableStripeBinding,
    actor: String,
    api_version: String,
    proof: Value,
    record: Value,
    customer: String,
}
impl CheckoutCreationReadSnapshot {
    #[must_use]
    pub fn provider_customer_id(&self) -> &str {
        &self.customer
    }
}
impl PostgresBillingStore {
    /// Resolves a completed, actor-owned creation receipt and current Customer
    /// relation. A mutable DTO/providerFields value can never establish this link.
    /// Returns None when no creation journal exists for this tenant/public ID.
    /// # Errors
    /// Rejects incomplete provenance, changed records/registrations, scope and actor mismatch.
    pub async fn checkout_creation_read_snapshot(
        &self,
        binding: &DurableStripeBinding,
        api_version: &str,
    ) -> BlueprintResult<Option<CheckoutCreationReadSnapshot>> {
        if binding.tenant != self.tenant.id() || binding.kind != BillingKind::CheckoutSession {
            return Err(conflict());
        }
        let actor = self.binding_actor()?.to_owned();
        let mut tx = self.transaction().await?;
        let operation: Option<String> = sqlx::query_scalar("SELECT operation_id::text FROM fl_billing.checkout_creation_intents WHERE tenant_id=$1 AND public_id=$2::uuid")
            .bind(self.tenant.id()).bind(binding.public_id.to_string())
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let Some(operation) = operation else {
            tx.commit().await.map_err(db_error)?;
            return Ok(None);
        };
        let operation = Uuid::parse_str(&operation).map_err(|_| storage_error())?;
        let row = self
            .checkout_intent_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        let intent = self.open_checkout_intent(operation, &row)?;
        let receipt = self.open_checkout_receipt(operation, &row)?;
        if intent["tenant"] != self.tenant.id()
            || intent["actor"] != actor
            || intent["deployment"] != binding.scope.deployment_label
            || intent["account"] != binding.scope.account_id
            || intent["livemode"] != binding.scope.livemode
            || intent["apiVersion"] != api_version
            || receipt["id"] != binding.storage_id
            || receipt["customerId"] != intent["customer"]
            || receipt["providerFields"]["id"] != binding.provider_id
        {
            return Err(conflict());
        }
        let customer = intent["customer"].as_str().ok_or_else(storage_error)?;
        // One statement observes Checkout, Customer, both binding generations and
        // immutable journal together. No lock is held during provider I/O.
        let observed: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('record',r.data,'recordVersion',r.xmin::text,'customerData',c.data,'customerVersion',c.xmin::text,'customerBinding',to_jsonb(cb)) FROM fl_billing.provider_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id JOIN fl_billing.records c ON c.tenant_id=b.tenant_id AND c.scope='durable' AND c.kind='customer' AND c.id=$10 JOIN fl_billing.stripe_customer_bindings cb ON cb.tenant_id=c.tenant_id AND cb.storage_id=c.id AND cb.local_customer_id::text=c.id AND cb.deployment_label=b.deployment_label AND cb.account_id=b.account_id AND cb.livemode=b.livemode AND cb.verified_api_version=$11 JOIN fl_billing.checkout_creation_intents j ON j.tenant_id=b.tenant_id AND j.public_id=b.public_id AND j.operation_id=$12::uuid AND j.state='completed' AND j.intent_ciphertext=$13 AND j.receipt_ciphertext=$14 WHERE b.tenant_id=$1 AND b.kind='checkout_session' AND b.public_id=$2::uuid AND b.binding_id=$3::uuid AND b.storage_id=$4 AND b.provider_id=$5 AND b.deployment_label=$6 AND b.account_id=$7 AND b.livemode=$8 AND r.data->>'id'=$4 AND c.data->>'id'=$10 AND j.payload_hash=$9")
            .bind(self.tenant.id()).bind(binding.public_id.to_string()).bind(binding.binding_id.to_string()).bind(&binding.storage_id).bind(&binding.provider_id).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(&row.2).bind(customer).bind(api_version).bind(operation.to_string()).bind(&row.3).bind(&row.5)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let observed = observed.ok_or_else(conflict)?.0;
        let dependencies = &intent["dependencies"];
        for key in ["customerData", "customerVersion", "customerBinding"] {
            if observed[key] != dependencies[key] {
                return Err(conflict());
            }
        }
        let record = self
            .decode(
                &mut tx,
                Scope::Durable,
                BillingKind::CheckoutSession,
                &binding.storage_id,
                observed["record"].clone(),
                "checkout_creation_read",
            )
            .await?;
        if record != receipt {
            return Err(conflict());
        }
        let provider_customer = dependencies["customerBinding"]["provider_customer_id"]
            .as_str()
            .ok_or_else(storage_error)?
            .to_owned();
        tx.commit().await.map_err(db_error)?;
        Ok(Some(CheckoutCreationReadSnapshot {
            binding: binding.clone(),
            actor,
            api_version: api_version.into(),
            proof: json!({"observed":observed,"intent":row.3,"receipt":row.5}),
            record,
            customer: provider_customer,
        }))
    }
    /// Rechecks the complete snapshot after network I/O. Caller must check its
    /// provider-context deadline after this awaited read. No financial authority.
    /// # Errors
    /// Denies changed Customer/Checkout generation, actor, receipt or local body.
    pub async fn read_checkout_creation_snapshot(
        &self,
        snapshot: &CheckoutCreationReadSnapshot,
    ) -> BlueprintResult<Value> {
        if snapshot.actor != self.binding_actor()? {
            return Err(conflict());
        }
        let current = self
            .checkout_creation_read_snapshot(&snapshot.binding, &snapshot.api_version)
            .await?
            .ok_or_else(conflict)?;
        if current.proof != snapshot.proof {
            return Err(conflict());
        }
        Ok(current.record)
    }
}
