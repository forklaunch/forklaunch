//! Append-only local revocation, independent of provider URL validity.
use super::{PostgresBillingStore, StripeBindingScope, conflict, db_error, storage_error};
use forklaunch_core::domain::BlueprintResult;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn profile_hash(
    scope: &StripeBindingScope,
    api: &str,
    config: &str,
    redirect: &str,
) -> String {
    let encoded = serde_json::json!([
        "portal_local_expiration_v1",
        scope.deployment_label,
        scope.account_id,
        scope.livemode,
        api,
        config,
        redirect
    ])
    .to_string();
    base16ct::lower::encode_string(&Sha256::digest(encoded.as_bytes()))
}
impl PostgresBillingStore {
    /// Validates the migration-16 read grant required by every stored Portal GET.
    /// This grants no local expiration or provider mutation authority.
    /// # Errors
    /// Denies missing schema/table access before a selected Portal listener starts.
    pub async fn validate_portal_expiration_read_access(&self) -> BlueprintResult<()> {
        let allowed:bool=sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE') AND has_table_privilege(current_user,'fl_billing.portal_local_expirations','SELECT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    /// Validates local expiration grants without provider I/O or publisher authority.
    /// # Errors
    /// Denies missing lock, read or append permissions and unsafe owner roles.
    pub async fn validate_portal_local_expiration_access(&self) -> BlueprintResult<()> {
        let allowed:bool=sqlx::query_scalar("SELECT has_table_privilege(current_user,'fl_billing.records','SELECT') AND has_table_privilege(current_user,'fl_billing.records','UPDATE') AND has_table_privilege(current_user,'fl_billing.provider_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.provider_bindings','UPDATE') AND has_table_privilege(current_user,'fl_billing.stripe_customer_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.stripe_customer_bindings','UPDATE') AND has_table_privilege(current_user,'fl_billing.portal_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.portal_local_expirations','SELECT') AND has_table_privilege(current_user,'fl_billing.portal_local_expirations','INSERT') AND has_table_privilege(current_user,'fl_billing.sensitive_read_audit','INSERT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        self.portal_runtime_role(&mut tx).await?;
        tx.rollback().await.map_err(db_error)
    }
    pub(super) async fn portal_expiration_present(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        public: Uuid,
    ) -> BlueprintResult<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_billing.portal_local_expirations WHERE tenant_id=$1 AND public_id=$2::uuid)").bind(self.tenant.id()).bind(public.to_string()).fetch_one(&mut **tx).await.map_err(db_error)
    }
    /// Historical actor/profile-bound local status. A missing receipt is not proof
    /// that an uncertain transaction rolled back. No current Customer or URL claim.
    /// # Errors
    /// Denies foreign provenance, actor or trusted configuration.
    pub async fn inspect_portal_local_expiration(
        &self,
        scope: &StripeBindingScope,
        public: Uuid,
        api: &str,
        config: &str,
        redirect: &str,
    ) -> BlueprintResult<bool> {
        let mut tx = self.transaction().await?;
        self.portal_runtime_role(&mut tx).await?;
        let operation:Option<String>=sqlx::query_scalar("SELECT operation_id::text FROM fl_billing.portal_creation_intents WHERE tenant_id=$1 AND public_id=$2::uuid AND state='completed'").bind(self.tenant.id()).bind(public.to_string()).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let operation =
            Uuid::parse_str(&operation.ok_or_else(conflict)?).map_err(|_| storage_error())?;
        let row = self
            .portal_intent_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        let intent = self.open_portal_intent(operation, &row)?;
        let receipt = self.open_portal_receipt(operation, &row)?;
        if intent["tenant"] != self.tenant.id()
            || intent["deployment"] != scope.deployment_label
            || intent["account"] != scope.account_id
            || intent["livemode"] != scope.livemode
            || intent["apiVersion"] != api
            || intent["payload"]["profile"] != "direct_portal_v1"
            || intent["payload"]["fields"]["configuration"] != config
            || intent["payload"]["fields"]["return_url"] != redirect
            || receipt["id"] != public.to_string()
        {
            return Err(conflict());
        }
        let stored:Option<(String,String,String)>=sqlx::query_as("SELECT actor_id,profile_hash,operation_id::text FROM fl_billing.portal_local_expirations WHERE tenant_id=$1 AND public_id=$2::uuid").bind(self.tenant.id()).bind(public.to_string()).fetch_optional(&mut *tx).await.map_err(db_error)?;
        if let Some((actor, hash, op)) = &stored
            && (actor != self.binding_actor()?
                || hash != &profile_hash(scope, api, config, redirect)
                || op != &operation.to_string())
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(stored.is_some())
    }
}
