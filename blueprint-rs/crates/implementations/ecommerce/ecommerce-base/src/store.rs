//! Atomic persistence boundary shared by memory and `PostgreSQL` implementations.
mod compliance;
mod retention;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{
    CartDto, CartItemDto, InventoryDto, OrderDto, OrderStatus, PaymentDto, PaymentStatus,
    ProductDto, UpdateProductDto, UpdateVariantDto, VariantDto, events::OrderEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, sync::Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordKind {
    Product,
    Variant,
    Inventory,
    Cart,
    Order,
    Payment,
}
impl RecordKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Variant => "variant",
            Self::Inventory => "inventory",
            Self::Cart => "cart",
            Self::Order => "order",
            Self::Payment => "payment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "lowercase")]
pub enum Record {
    Product(ProductDto),
    Variant(VariantDto),
    Inventory(InventoryDto),
    Cart(CartDto),
    Order(OrderDto),
    Payment(PaymentDto),
}
macro_rules! record_methods {
    ($($kind:ident),+ $(,)?) => {
        impl Record {
            #[must_use]
            pub const fn kind(&self) -> RecordKind { match self { $(Self::$kind(_)=>RecordKind::$kind),+ } }
            #[must_use]
            pub fn id(&self) -> &str { match self { $(Self::$kind(value)=>&value.id),+ } }
            pub(crate) fn data(&self) -> BlueprintResult<Value> { match self { $(Self::$kind(value)=>serde_json::to_value(value).map_err(storage_error)),+ } }
            pub(crate) fn from_data(kind:RecordKind, value:Value) -> BlueprintResult<Self> { match kind { $(RecordKind::$kind=>serde_json::from_value(value).map(Self::$kind).map_err(storage_error)),+ } }
            fn touch(&mut self) { let now=crate::timestamp(); match self { $(Self::$kind(value)=>value.updated_at=Some(now)),+ } }
        }
    };
}
record_methods!(Product, Variant, Inventory, Cart, Order, Payment);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Id,
    ExternalId,
    Handle,
    ProductId,
    VariantId,
    ProviderRef,
    CustomerId,
    CartId,
    OrderId,
}
impl Field {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::ExternalId => "externalId",
            Self::Handle => "handle",
            Self::ProductId => "productId",
            Self::VariantId => "variantId",
            Self::ProviderRef => "providerRef",
            Self::CustomerId => "customerId",
            Self::CartId => "cartId",
            Self::OrderId => "orderId",
        }
    }
}
#[derive(Debug, Clone)]
pub struct Selector {
    pub kind: RecordKind,
    pub field: Field,
    pub value: String,
}
impl Selector {
    #[must_use]
    pub fn new(kind: RecordKind, field: Field, value: impl Into<String>) -> Self {
        Self {
            kind,
            field,
            value: value.into(),
        }
    }
    pub(crate) fn validate_unique(&self) -> BlueprintResult<()> {
        let valid = self.field == Field::Id
            || matches!(
                (self.kind, self.field),
                (RecordKind::Product, Field::ExternalId | Field::Handle)
                    | (RecordKind::Variant, Field::ExternalId)
                    | (RecordKind::Inventory, Field::VariantId)
                    | (RecordKind::Payment, Field::ProviderRef)
            );
        if valid {
            Ok(())
        } else {
            Err(validation("Mutation requires a unique record selector"))
        }
    }
    #[must_use]
    pub fn id(kind: RecordKind, id: impl Into<String>) -> Self {
        Self::new(kind, Field::Id, id)
    }
}
#[derive(Debug, Clone)]
pub struct Query {
    pub kind: RecordKind,
    pub ids: Option<Vec<String>>,
    pub equals: Option<(Field, String)>,
    pub equals_many: Option<(Field, Vec<String>)>,
    pub title: Option<String>,
}
impl Query {
    #[must_use]
    pub const fn all(kind: RecordKind) -> Self {
        Self {
            kind,
            ids: None,
            equals: None,
            equals_many: None,
            title: None,
        }
    }
    #[must_use]
    pub fn selected(selector: Selector) -> Self {
        Self {
            kind: selector.kind,
            equals: Some((selector.field, selector.value)),
            ..Self::all(selector.kind)
        }
    }
}
#[derive(Debug, Clone)]
pub enum Mutation {
    PatchProduct(UpdateProductDto),
    PatchVariant(UpdateVariantDto),
    AdjustStock(i64),
    AddCartItem {
        variant_id: String,
        quantity: i64,
    },
    RemoveCartItem(String),
    ClearCart,
    TransitionOrder {
        expected: OrderStatus,
        to: OrderStatus,
    },
    ConfirmPayment,
    FailPayment,
    ClaimConfirmation,
    ReleaseConfirmation,
}
#[derive(Debug, Clone)]
pub struct MutationResult {
    pub record: Record,
    pub changed: bool,
}

/// Bounded product rows in stable createdAt/id order, with the pre-page total.
#[derive(Debug, Clone)]
pub struct ProductPage {
    pub products: Vec<ProductDto>,
    pub total: u64,
}

/// Every mutation must atomically check and persist the current row. Returning
/// plaintext order data also requires the backend's audit to commit first.
pub trait EcommerceStore: Send + Sync {
    /// # Errors
    /// Returns storage errors, including unique-key violations.
    fn insert(&self, record: Record) -> impl Future<Output = BlueprintResult<Record>> + Send;
    /// # Errors
    /// Returns storage/audit failures; results must remain within the trusted tenant.
    fn find(&self, query: Query) -> impl Future<Output = BlueprintResult<Vec<Record>>> + Send;
    /// Implementations must page before loading product rows into the process.
    /// Empty ids mean no restriction, matching TypeScript's catalog lookup.
    /// # Errors
    /// Returns storage failures. Limits must remain within 1..=100.
    fn product_page(
        &self,
        ids: Option<Vec<String>>,
        title: Option<String>,
        limit: u32,
        offset: u64,
    ) -> impl Future<Output = BlueprintResult<ProductPage>> + Send;
    /// Atomically applies all order-event inventory deltas and an idempotency
    /// receipt. Reused IDs with different logical payloads must fail.
    /// # Errors
    /// Returns invalid-event, insufficient-stock, conflict or storage errors.
    fn apply_order_event(
        &self,
        event: OrderEvent,
    ) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns missing-record, invalid transition, stock/quantity or storage failures.
    fn mutate(
        &self,
        selector: Selector,
        mutation: Mutation,
    ) -> impl Future<Output = BlueprintResult<MutationResult>> + Send;
    /// # Errors
    /// Returns storage failures. Missing rows are an idempotent success.
    fn delete(&self, selector: Selector) -> impl Future<Output = BlueprintResult<()>> + Send;
}

#[must_use]
pub fn not_found(kind: RecordKind) -> BlueprintError {
    BlueprintError::new(
        "ecommerce_not_found",
        format!("{} record not found", kind.as_str()),
    )
}
pub(crate) fn storage_error(_: impl std::fmt::Display) -> BlueprintError {
    BlueprintError::new(
        "ecommerce_storage_error",
        "Ecommerce storage operation failed",
    )
}
pub(crate) fn illegal_transition(from: OrderStatus, to: OrderStatus) -> BlueprintError {
    BlueprintError::new(
        "ecommerce_illegal_transition",
        format!(
            "Illegal order transition: {} -> {}",
            from.as_str(),
            to.as_str()
        ),
    )
}
fn validation(message: &str) -> BlueprintError {
    BlueprintError::new("ecommerce_validation", message)
}
fn patch<T: Serialize + serde::de::DeserializeOwned>(
    current: &mut T,
    update: impl Serialize,
) -> BlueprintResult<()> {
    let mut value = serde_json::to_value(&*current).map_err(storage_error)?;
    let update = serde_json::to_value(update).map_err(storage_error)?;
    let fields = value
        .as_object_mut()
        .ok_or_else(|| storage_error("record not object"))?;
    for (key, value) in update
        .as_object()
        .ok_or_else(|| storage_error("patch not object"))?
    {
        if key != "id" {
            fields.insert(key.clone(), value.clone());
        }
    }
    *current = serde_json::from_value(value).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn apply_mutation(
    mut record: Record,
    mutation: Mutation,
) -> BlueprintResult<MutationResult> {
    let mut changed = true;
    match (&mut record, mutation) {
        (Record::Product(value), Mutation::PatchProduct(update)) => patch(value, update)?,
        (Record::Variant(value), Mutation::PatchVariant(update)) => patch(value, update)?,
        (Record::Inventory(value), Mutation::AdjustStock(delta)) => {
            if delta < 0
                && value.stock
                    < delta
                        .checked_neg()
                        .ok_or_else(|| validation("Stock adjustment overflow"))?
            {
                return Err(BlueprintError::new(
                    "ecommerce_insufficient_stock",
                    format!(
                        "Insufficient stock for variant {}: have {}, requested change {delta}",
                        value.variant_id, value.stock
                    ),
                ));
            }
            value.stock = value
                .stock
                .checked_add(delta)
                .ok_or_else(|| validation("Stock adjustment overflow"))?;
        }
        (
            Record::Cart(value),
            Mutation::AddCartItem {
                variant_id,
                quantity,
            },
        ) => {
            if quantity < 1 {
                return Err(validation("Cart item quantity must be a positive integer"));
            }
            if let Some(item) = value
                .items
                .iter_mut()
                .find(|item| item.variant_id == variant_id)
            {
                item.quantity = item
                    .quantity
                    .checked_add(quantity)
                    .ok_or_else(|| validation("Cart quantity overflow"))?;
            } else {
                value.items.push(CartItemDto {
                    variant_id,
                    quantity,
                });
            }
        }
        (Record::Cart(value), Mutation::RemoveCartItem(variant_id)) => {
            value.items.retain(|item| item.variant_id != variant_id);
        }
        (Record::Cart(value), Mutation::ClearCart) => value.items.clear(),
        (Record::Order(value), Mutation::TransitionOrder { expected, to }) => {
            if value.status != expected || !value.status.can_transition_to(to) {
                return Err(illegal_transition(value.status, to));
            }
            value.status = to;
        }
        (Record::Payment(value), Mutation::ConfirmPayment) => {
            changed = value.status != PaymentStatus::Succeeded;
            value.status = PaymentStatus::Succeeded;
        }
        (Record::Payment(value), Mutation::FailPayment) => {
            changed = value.status != PaymentStatus::Succeeded;
            if changed {
                value.status = PaymentStatus::Failed;
            }
        }
        (Record::Payment(value), Mutation::ClaimConfirmation) => {
            changed = value.status == PaymentStatus::Pending;
            if changed {
                value.status = PaymentStatus::Succeeded;
            }
        }
        (Record::Payment(value), Mutation::ReleaseConfirmation) => {
            changed = value.status == PaymentStatus::Succeeded;
            if changed {
                value.status = PaymentStatus::Pending;
            }
        }
        _ => return Err(storage_error("mutation kind mismatch")),
    }
    if changed {
        record.touch();
    }
    Ok(MutationResult { record, changed })
}

/// Development adapter; each instance is an isolated logical tenant. Production
/// applications should use the `PostgreSQL` adapter and restricted database roles.
#[derive(Debug, Default)]
pub struct InMemoryEcommerceStore {
    pub(crate) records: Mutex<BTreeMap<(RecordKind, String), Record>>,
    pub(crate) payment_reconciliation_audit:
        Mutex<Vec<crate::reconciliation::PaymentReconciliationAudit>>,
    pub(crate) payment_cancellation_audit:
        Mutex<Vec<crate::cancellation::PaymentCancellationAudit>>,
    applied_events: Mutex<BTreeMap<String, String>>,
    retention_tenant: Option<forklaunch_core::persistence::TenantContext>,
    retention_markers: Mutex<std::collections::BTreeSet<String>>,
}
impl InMemoryEcommerceStore {
    /// Bind development retention operations to one logical tenant.
    #[must_use]
    pub fn for_tenant(tenant: forklaunch_core::persistence::TenantContext) -> Self {
        Self {
            retention_tenant: Some(tenant),
            ..Self::default()
        }
    }
}
fn unique(record: &Record) -> Vec<(Field, &str)> {
    match record {
        Record::Product(value) => vec![
            (Field::ExternalId, &value.external_id),
            (Field::Handle, &value.handle),
        ],
        Record::Variant(value) => vec![(Field::ExternalId, &value.external_id)],
        Record::Inventory(value) => vec![(Field::VariantId, &value.variant_id)],
        Record::Payment(value) => value
            .provider_ref
            .as_deref()
            .map(|id| vec![(Field::ProviderRef, id)])
            .unwrap_or_default(),
        Record::Order(value) if value.status == OrderStatus::Pending => value
            .cart_id
            .as_deref()
            .map(|id| vec![(Field::CartId, id)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}
fn check_unique(
    records: &BTreeMap<(RecordKind, String), Record>,
    candidate: &Record,
) -> BlueprintResult<()> {
    for existing in records
        .values()
        .filter(|record| record.kind() == candidate.kind() && record.id() != candidate.id())
    {
        for (field, value) in unique(candidate) {
            if unique(existing)
                .iter()
                .any(|(existing_field, existing_value)| {
                    *existing_field == field && *existing_value == value
                })
            {
                return Err(BlueprintError::new(
                    "ecommerce_conflict",
                    "Ecommerce unique key already exists",
                ));
            }
        }
    }
    Ok(())
}
fn matches(record: &Record, query: &Query) -> BlueprintResult<bool> {
    if record.kind() != query.kind {
        return Ok(false);
    }
    if query
        .ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty() && !ids.iter().any(|id| id == record.id()))
    {
        return Ok(false);
    }
    let data = record.data()?;
    if query
        .equals
        .as_ref()
        .is_some_and(|(field, value)| data[field.as_str()].as_str() != Some(value.as_str()))
    {
        return Ok(false);
    }
    if query.equals_many.as_ref().is_some_and(|(field, values)| {
        !data[field.as_str()]
            .as_str()
            .is_some_and(|value| values.iter().any(|candidate| candidate == value))
    }) {
        return Ok(false);
    }
    if let Some(title) = query.title.as_deref().filter(|value| !value.is_empty()) {
        let pattern = format!(
            "(?is).*{}.*",
            regex::escape(title).replace('%', ".*").replace('_', ".")
        );
        let regex = regex::Regex::new(&pattern).map_err(storage_error)?;
        if !data["title"]
            .as_str()
            .is_some_and(|title| regex.is_match(title))
        {
            return Ok(false);
        }
    }
    Ok(true)
}
impl EcommerceStore for InMemoryEcommerceStore {
    async fn insert(&self, record: Record) -> BlueprintResult<Record> {
        Record::from_data(record.kind(), record.data()?)?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let key = (record.kind(), record.id().to_owned());
        if records.contains_key(&key) {
            return Err(BlueprintError::new(
                "ecommerce_conflict",
                "Ecommerce record already exists",
            ));
        }
        check_unique(&records, &record)?;
        records.insert(key, record.clone());
        Ok(record)
    }
    async fn find(&self, query: Query) -> BlueprintResult<Vec<Record>> {
        let records = self.records.lock().map_err(storage_error)?;
        records
            .values()
            .filter_map(|record| match matches(record, &query) {
                Ok(true) => Some(Ok(record.clone())),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }
    async fn product_page(
        &self,
        ids: Option<Vec<String>>,
        title: Option<String>,
        limit: u32,
        offset: u64,
    ) -> BlueprintResult<ProductPage> {
        if !(1..=100).contains(&limit) {
            return Err(validation("Catalog limit must be within 1..=100"));
        }
        let records = self.records.lock().map_err(storage_error)?;
        let query = Query {
            ids,
            title,
            ..Query::all(RecordKind::Product)
        };
        let mut products = Vec::new();
        for record in records.values() {
            if matches(record, &query)?
                && let Record::Product(product) = record
            {
                products.push(product);
            }
        }
        products.sort_by(|a, b| {
            a.created_at
                .is_none()
                .cmp(&b.created_at.is_none())
                .then(a.created_at.cmp(&b.created_at))
                .then(a.id.cmp(&b.id))
        });
        let total = products.len() as u64;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(products.len());
        let products = products
            .into_iter()
            .skip(start)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(ProductPage { products, total })
    }
    async fn apply_order_event(&self, event: OrderEvent) -> BlueprintResult<bool> {
        let deltas = crate::events::deltas(&event)?;
        let fingerprint = crate::events::fingerprint(&event)?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let mut receipts = self.applied_events.lock().map_err(storage_error)?;
        if let Some(previous) = receipts.get(&event.id) {
            return if previous == &fingerprint {
                Ok(false)
            } else {
                Err(crate::events::conflicting_event())
            };
        }
        let mut updated = Vec::new();
        for (variant, delta) in deltas {
            let record = records.values().find(|record| matches!(record, Record::Inventory(value) if value.variant_id == variant))
                .ok_or_else(|| not_found(RecordKind::Inventory))?;
            let result = apply_mutation(record.clone(), Mutation::AdjustStock(delta))?;
            updated.push(result.record);
        }
        for record in updated {
            records.insert((record.kind(), record.id().to_owned()), record);
        }
        receipts.insert(event.id, fingerprint);
        Ok(true)
    }
    async fn mutate(
        &self,
        selector: Selector,
        mutation: Mutation,
    ) -> BlueprintResult<MutationResult> {
        selector.validate_unique()?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let query = Query::selected(selector);
        let mut old = None;
        for record in records.values() {
            if matches(record, &query)? {
                old = Some(record.clone());
                break;
            }
        }
        let old = old.ok_or_else(|| not_found(query.kind))?;
        if matches!(
            mutation,
            Mutation::ConfirmPayment | Mutation::ClaimConfirmation
        ) && let Record::Payment(payment) = &old
            && self
                .payment_cancellation_audit
                .lock()
                .map_err(storage_error)?
                .iter()
                .any(|audit| {
                    audit.payment_id == payment.id
                        && Some(&audit.evidence.provider_ref) == payment.provider_ref.as_ref()
                })
        {
            return Err(crate::cancellation::conflict());
        }
        if matches!(mutation, Mutation::ReleaseConfirmation)
            && let Record::Payment(payment) = &old
            && self
                .payment_reconciliation_audit
                .lock()
                .map_err(storage_error)?
                .iter()
                .any(|audit| {
                    audit.payment_id == payment.id
                        && Some(audit.evidence.provider_ref.as_str())
                            == payment.provider_ref.as_deref()
                })
        {
            return Ok(MutationResult {
                record: old,
                changed: false,
            });
        }
        let result = apply_mutation(old, mutation)?;
        Record::from_data(result.record.kind(), result.record.data()?)?;
        check_unique(&records, &result.record)?;
        records.insert(
            (result.record.kind(), result.record.id().to_owned()),
            result.record.clone(),
        );
        Ok(result)
    }
    async fn delete(&self, selector: Selector) -> BlueprintResult<()> {
        let mut records = self.records.lock().map_err(storage_error)?;
        let query = Query::selected(selector);
        let keys: BlueprintResult<Vec<_>> = records
            .iter()
            .filter_map(|(key, record)| match matches(record, &query) {
                Ok(true) => Some(Ok(key.clone())),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect();
        let mut markers = self.retention_markers.lock().map_err(storage_error)?;
        for key in keys? {
            if key.0 == RecordKind::Order {
                markers.remove(&key.1);
            }
            records.remove(&key);
        }
        Ok(())
    }
}
