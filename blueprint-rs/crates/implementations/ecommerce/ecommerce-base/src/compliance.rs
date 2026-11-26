//! Explicit subject mapping for the configured Order shipping-address classification.
//! Stores own tenant scope and commit decrypted reads and erasure atomically.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::ShippingAddressDto;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{collections::BTreeMap, future::Future, sync::Arc};
use zeroize::Zeroizing;

/// The TS generic walk falls back to `id` before `customerId`. Applications that
/// identify customers must explicitly select the customer mapping on both sides.
#[derive(Debug, Clone, Copy, Default)]
pub enum OrderSubjectField {
    #[default]
    Id,
    CustomerId,
}
#[cfg(feature = "postgres")]
impl OrderSubjectField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::CustomerId => "customerId",
        }
    }
}
pub struct AddressAccess<'a> {
    pub order_id: &'a str,
    pub field: &'static str,
    pub level: &'static str,
}
/// Classified values serialize as opaque objects, matching TS `CompliantField`.
/// No Debug or implicit plaintext conversion is provided.
pub struct ProtectedAddress {
    encoded: Zeroizing<String>,
    order_id: String,
}
impl ProtectedAddress {
    /// Explicit plaintext access requires the caller's trusted audit sink.
    /// # Errors
    /// Returns a sanitized error if the protected representation is corrupt.
    pub fn deanon(
        &self,
        audit: impl FnOnce(AddressAccess<'_>),
    ) -> BlueprintResult<ShippingAddressDto> {
        audit(AddressAccess {
            order_id: &self.order_id,
            field: "Order.shippingAddress",
            level: "pii",
        });
        serde_json::from_str(&self.encoded).map_err(crate::store::storage_error)
    }
}
impl Serialize for ProtectedAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_map(Some(0))?.end()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderExport {
    pub id: String,
    pub shipping_address: ProtectedAddress,
}
impl OrderExport {
    pub(crate) fn new(id: String, address: &ShippingAddressDto) -> BlueprintResult<Self> {
        Ok(Self {
            shipping_address: ProtectedAddress {
                encoded: Zeroizing::new(
                    serde_json::to_string(address).map_err(crate::store::storage_error)?,
                ),
                order_id: id.clone(),
            },
            id,
        })
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityFailure {
    pub entity_name: String,
    pub reason: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub user_id: String,
    pub entities: BTreeMap<String, Vec<OrderExport>>,
    pub failures: Vec<EntityFailure>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseResult {
    pub entities_affected: Vec<String>,
    pub records_deleted: usize,
    pub failures: Vec<EntityFailure>,
}
/// Trusted, tenant-bound storage capability. A failed read, decrypt, audit or
/// commit must expose no partial export and commit no partial order erasure.
pub trait EcommerceComplianceStore: Send + Sync {
    /// # Errors
    /// Returns sanitized storage, decryption or audit errors.
    fn export_orders(
        &self,
        subject: &str,
        field: OrderSubjectField,
    ) -> impl Future<Output = BlueprintResult<Vec<OrderExport>>> + Send;
    /// Delete whole classified records, matching the generic TypeScript walker.
    /// Unclassified carts/payments/events are not cascaded by this operation.
    /// # Errors
    /// Returns sanitized storage, decryption or audit errors.
    fn erase_orders(
        &self,
        subject: &str,
        field: OrderSubjectField,
    ) -> impl Future<Output = BlueprintResult<usize>> + Send;
}
pub struct EcommerceComplianceService<S> {
    store: Arc<S>,
    subject_field: OrderSubjectField,
}
impl<S: EcommerceComplianceStore> EcommerceComplianceService<S> {
    #[must_use]
    pub fn new(store: Arc<S>, subject_field: OrderSubjectField) -> Self {
        Self {
            store,
            subject_field,
        }
    }
    /// Export only Order ID and classified shipping address.
    /// # Errors
    /// Rejects invalid subjects. Nonempty failures indicate incomplete export.
    pub async fn export(&self, subject: &str) -> BlueprintResult<ExportResult> {
        validate_subject(subject)?;
        let mut result = ExportResult {
            user_id: subject.into(),
            entities: BTreeMap::new(),
            failures: Vec::new(),
        };
        match self.store.export_orders(subject, self.subject_field).await {
            Ok(rows) if !rows.is_empty() => {
                result.entities.insert("Order".into(), rows);
            }
            Ok(_) => {}
            Err(_) => result.failures.push(failure()),
        }
        Ok(result)
    }
    /// # Errors
    /// Rejects invalid subjects. Nonempty failures indicate incomplete erasure.
    pub async fn erase(&self, subject: &str) -> BlueprintResult<EraseResult> {
        validate_subject(subject)?;
        let mut result = EraseResult::default();
        match self.store.erase_orders(subject, self.subject_field).await {
            Ok(count) if count > 0 => {
                result.records_deleted = count;
                result.entities_affected.push("Order".into());
            }
            Ok(_) => {}
            Err(_) => result.failures.push(failure()),
        }
        Ok(result)
    }
}
pub(crate) fn validate_subject(subject: &str) -> BlueprintResult<()> {
    if subject.trim().is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        return Err(BlueprintError::new(
            "ecommerce_compliance_invalid",
            "Invalid compliance subject",
        ));
    }
    Ok(())
}
fn failure() -> EntityFailure {
    EntityFailure {
        entity_name: "Order".into(),
        reason: "Unable to complete ecommerce compliance operation".into(),
    }
}
