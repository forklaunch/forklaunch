//! Persistent ecommerce business rules with atomic storage seams. The memory
//! adapter is for development; `PostgreSQL` owns production tenant isolation.
#![forbid(unsafe_code)]
pub mod cancellation;
pub mod catalog;
pub mod checkout;
pub mod compliance;
pub mod events;
pub mod import;
pub mod payment_resume;
pub mod payment_review;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod reconciliation;
pub mod retention;
pub mod shipping;
pub mod storage_compliance;
pub mod store;
pub mod webhook;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{
    AddCartItemDto, AdjustStockDto, CartDto, CartService, ConfirmPaymentDto, CreateCartDto,
    CreateInventoryDto, CreateOrderDto, CreatePaymentDto, CreateProductDto, CreateVariantDto,
    FailPaymentDto, InventoryDto, InventoryService, OrderDto, OrderService, OrderStatus,
    PaymentDto, PaymentService, PaymentStatus, ProductDto, ProductSearchDto, ProductService,
    RemoveCartItemDto, StockCheckDto, StockCheckResultDto, TransitionOrderDto, UpdateProductDto,
    UpdateVariantDto, VariantDto, VariantService,
};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use store::{EcommerceStore, Field, Mutation, Query, Record, RecordKind, Selector};

#[must_use]
pub fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn id(value: Option<String>) -> String {
    value.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}
fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_storage_error",
        "Unexpected ecommerce record kind",
    )
}

macro_rules! record_conversion {
    ($($kind:ident=>$dto:ident),+ $(,)?) => {$ (
        impl TryFrom<Record> for $dto {
            type Error=BlueprintError;
            fn try_from(record:Record) -> BlueprintResult<Self> {
                if let Record::$kind(value)=record {Ok(value)} else {Err(mismatch())}
            }
        }
    )+};
}
record_conversion!(Product=>ProductDto,Variant=>VariantDto,Inventory=>InventoryDto,Cart=>CartDto,Order=>OrderDto,Payment=>PaymentDto);

pub type CacheFuture<'a, T> = Pin<Box<dyn Future<Output = BlueprintResult<T>> + Send + 'a>>;
/// Optional read-through cart cache. The adapter owns its tenant namespace and
/// TTL; cache misses/errors/timeouts always fall back to persistent storage.
pub trait CartCache: Send + Sync {
    /// # Errors
    /// Returns cache failures; the service treats them as cache misses.
    fn read(&self, id: &str) -> CacheFuture<'_, Option<CartDto>>;
    /// # Errors
    /// Returns cache failures; they never fail a completed persistent operation.
    fn write(&self, cart: CartDto) -> CacheFuture<'_, ()>;
}

pub struct BaseEcommerceService<S> {
    store: Arc<S>,
    cache: Option<Arc<dyn CartCache>>,
    cache_timeout: Duration,
}
impl<S> Clone for BaseEcommerceService<S> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            cache: self.cache.clone(),
            cache_timeout: self.cache_timeout,
        }
    }
}
impl<S: EcommerceStore> BaseEcommerceService<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            cache: None,
            cache_timeout: Duration::from_millis(750),
        }
    }
    #[must_use]
    pub fn with_cart_cache(mut self, cache: Arc<dyn CartCache>) -> Self {
        self.cache = Some(cache);
        self
    }
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }
    async fn one<T: TryFrom<Record, Error = BlueprintError>>(
        &self,
        selector: Selector,
    ) -> BlueprintResult<T> {
        let kind = selector.kind;
        self.store
            .find(Query::selected(selector))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| store::not_found(kind))?
            .try_into()
    }
    async fn many<T: TryFrom<Record, Error = BlueprintError>>(
        &self,
        query: Query,
    ) -> BlueprintResult<Vec<T>> {
        self.store
            .find(query)
            .await?
            .into_iter()
            .map(T::try_from)
            .collect()
    }
    async fn mutate<T: TryFrom<Record, Error = BlueprintError>>(
        &self,
        selector: Selector,
        mutation: Mutation,
    ) -> BlueprintResult<T> {
        self.store
            .mutate(selector, mutation)
            .await?
            .record
            .try_into()
    }
    async fn warm(&self, cart: &CartDto) {
        if let Some(cache) = &self.cache
            && !matches!(
                tokio::time::timeout(self.cache_timeout, cache.write(cart.clone())).await,
                Ok(Ok(()))
            )
        {
            tracing::warn!("Cart cache write failed or timed out");
        }
    }
    async fn mutate_cart(&self, id: String, mutation: Mutation) -> BlueprintResult<CartDto> {
        let cart = self
            .mutate(Selector::id(RecordKind::Cart, id), mutation)
            .await?;
        self.warm(&cart).await;
        Ok(cart)
    }
    /// Creates a pending order for a checkout cart. Storage enforces one pending
    /// order per cart; direct `create_order` calls omit the cart association.
    /// # Errors
    /// Returns storage/unique-key failures.
    pub async fn create_order_for_cart(
        &self,
        request: CreateOrderDto,
        cart_id: Option<String>,
    ) -> BlueprintResult<OrderDto> {
        let now = timestamp();
        let record = OrderDto {
            id: id(request.id),
            customer_id: request.customer_id,
            items: request.items,
            shipping_address: request.shipping_address,
            subtotal_cents: request.subtotal_cents,
            discount_cents: request.discount_cents,
            tax_cents: request.tax_cents,
            tax_breakdown: request.tax_breakdown,
            shipping_cents: request.shipping_cents,
            gift_card_cents: request.gift_card_cents,
            total_cents: request.total_cents,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            status: OrderStatus::Pending,
            cart_id,
        };
        self.store.insert(Record::Order(record)).await?.try_into()
    }
    /// Persists a pending payment using the reference returned by a provider.
    /// # Errors
    /// Returns storage/unique-key failures.
    pub async fn create_payment_with_reference(
        &self,
        request: CreatePaymentDto,
        provider_ref: Option<String>,
    ) -> BlueprintResult<PaymentDto> {
        let now = timestamp();
        let record = PaymentDto {
            id: id(request.id),
            order_id: request.order_id,
            amount_cents: request.amount_cents,
            currency: request.currency,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            status: PaymentStatus::Pending,
            provider_ref,
        };
        self.store.insert(Record::Payment(record)).await?.try_into()
    }
    /// Atomically claims pending -> succeeded before an external capture.
    /// # Errors
    /// Rejects audited terminal cancellation and storage failures. False means
    /// another caller already claimed it, it is missing, or it is not pending.
    pub async fn claim_for_confirmation(&self, provider_ref: &str) -> BlueprintResult<bool> {
        match self
            .store
            .mutate(
                Selector::new(RecordKind::Payment, Field::ProviderRef, provider_ref),
                Mutation::ClaimConfirmation,
            )
            .await
        {
            Ok(outcome) => Ok(outcome.changed),
            Err(error) if error.code == "ecommerce_not_found" => Ok(false),
            Err(error) => Err(error),
        }
    }
    /// Releases a succeeded confirmation claim back to pending after a failed capture.
    /// # Errors
    /// Returns storage failures. A missing reference is an idempotent no-op.
    pub async fn release_confirmation_claim(&self, provider_ref: &str) -> BlueprintResult<()> {
        match self
            .store
            .mutate(
                Selector::new(RecordKind::Payment, Field::ProviderRef, provider_ref),
                Mutation::ReleaseConfirmation,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if error.code == "ecommerce_not_found" => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl<S: EcommerceStore> CartService for BaseEcommerceService<S> {
    async fn create_cart(&self, request: CreateCartDto) -> BlueprintResult<CartDto> {
        let now = timestamp();
        let cart: CartDto = self
            .store
            .insert(Record::Cart(CartDto {
                id: id(request.id),
                customer_id: request.customer_id,
                status: "open".into(),
                items: Vec::new(),
                created_at: Some(now.clone()),
                updated_at: Some(now),
            }))
            .await?
            .try_into()?;
        self.warm(&cart).await;
        Ok(cart)
    }
    async fn get_cart(&self, id: &str) -> BlueprintResult<CartDto> {
        if let Some(cache) = &self.cache
            && let Ok(Ok(Some(cart))) =
                tokio::time::timeout(self.cache_timeout, cache.read(id)).await
            && cart.id == id
        {
            return Ok(cart);
        }
        let cart = self.one(Selector::id(RecordKind::Cart, id)).await?;
        self.warm(&cart).await;
        Ok(cart)
    }
    async fn add_item(&self, request: AddCartItemDto) -> BlueprintResult<CartDto> {
        if request.quantity < 1 {
            return Err(BlueprintError::new(
                "ecommerce_validation",
                "Cart item quantity must be a positive integer",
            ));
        }
        self.mutate_cart(
            request.cart_id,
            Mutation::AddCartItem {
                variant_id: request.variant_id,
                quantity: request.quantity,
            },
        )
        .await
    }
    async fn remove_item(&self, request: RemoveCartItemDto) -> BlueprintResult<CartDto> {
        self.mutate_cart(
            request.cart_id,
            Mutation::RemoveCartItem(request.variant_id),
        )
        .await
    }
    async fn clear_cart(&self, id: &str) -> BlueprintResult<()> {
        self.mutate_cart(id.into(), Mutation::ClearCart).await?;
        Ok(())
    }
}
impl<S: EcommerceStore> OrderService for BaseEcommerceService<S> {
    async fn create_order(&self, request: CreateOrderDto) -> BlueprintResult<OrderDto> {
        self.create_order_for_cart(request, None).await
    }
    async fn get_order(&self, id: &str) -> BlueprintResult<OrderDto> {
        self.one(Selector::id(RecordKind::Order, id)).await
    }
    async fn list_orders(&self, ids: Option<Vec<String>>) -> BlueprintResult<Vec<OrderDto>> {
        self.many(Query {
            ids,
            ..Query::all(RecordKind::Order)
        })
        .await
    }
    async fn transition_order(&self, request: TransitionOrderDto) -> BlueprintResult<OrderDto> {
        let current = self.get_order(&request.id).await?;
        if !current.status.can_transition_to(request.to) {
            return Err(store::illegal_transition(current.status, request.to));
        }
        self.mutate(
            Selector::id(RecordKind::Order, request.id),
            Mutation::TransitionOrder {
                expected: current.status,
                to: request.to,
            },
        )
        .await
    }
}
impl<S: EcommerceStore> PaymentService for BaseEcommerceService<S> {
    type CreatedPayment = PaymentDto;
    async fn create_payment(&self, request: CreatePaymentDto) -> BlueprintResult<PaymentDto> {
        self.create_payment_with_reference(request, None).await
    }
    async fn get_payment(&self, id: &str) -> BlueprintResult<PaymentDto> {
        self.one(Selector::id(RecordKind::Payment, id)).await
    }
    async fn confirm_payment(&self, request: ConfirmPaymentDto) -> BlueprintResult<PaymentDto> {
        self.mutate(
            Selector::new(
                RecordKind::Payment,
                Field::ProviderRef,
                request.provider_ref,
            ),
            Mutation::ConfirmPayment,
        )
        .await
    }
    async fn fail_payment(&self, request: FailPaymentDto) -> BlueprintResult<PaymentDto> {
        self.mutate(
            Selector::new(
                RecordKind::Payment,
                Field::ProviderRef,
                request.provider_ref,
            ),
            Mutation::FailPayment,
        )
        .await
    }
}
impl<S: EcommerceStore> ProductService for BaseEcommerceService<S> {
    async fn create_product(&self, request: CreateProductDto) -> BlueprintResult<ProductDto> {
        let now = timestamp();
        let record = ProductDto {
            id: id(request.id),
            external_id: request.external_id,
            handle: request.handle,
            source_url: request.source_url,
            title: request.title,
            description_html: request.description_html,
            vendor: request.vendor,
            product_type: request.product_type,
            tags: request.tags,
            options: request.options,
            images: request.images,
            created_at: Some(now.clone()),
            updated_at: Some(now),
        };
        self.store.insert(Record::Product(record)).await?.try_into()
    }
    async fn get_product(&self, id: &str) -> BlueprintResult<ProductDto> {
        self.one(Selector::id(RecordKind::Product, id)).await
    }
    async fn get_product_by_external_id(&self, external_id: &str) -> BlueprintResult<ProductDto> {
        self.one(Selector::new(
            RecordKind::Product,
            Field::ExternalId,
            external_id,
        ))
        .await
    }
    async fn update_product(&self, request: UpdateProductDto) -> BlueprintResult<ProductDto> {
        self.mutate(
            Selector::id(RecordKind::Product, request.id.clone()),
            Mutation::PatchProduct(request),
        )
        .await
    }
    async fn delete_product(&self, id: &str) -> BlueprintResult<()> {
        self.store
            .delete(Selector::id(RecordKind::Product, id))
            .await
    }
    async fn get_product_by_handle(&self, handle: &str) -> BlueprintResult<ProductDto> {
        self.one(Selector::new(RecordKind::Product, Field::Handle, handle))
            .await
    }
    async fn list_products(
        &self,
        search: Option<ProductSearchDto>,
    ) -> BlueprintResult<Vec<ProductDto>> {
        let search = search.unwrap_or_default();
        self.many(Query {
            ids: search.ids,
            title: search.title,
            ..Query::all(RecordKind::Product)
        })
        .await
    }
}
impl<S: EcommerceStore> VariantService for BaseEcommerceService<S> {
    async fn create_variant(&self, request: CreateVariantDto) -> BlueprintResult<VariantDto> {
        let now = timestamp();
        let record = VariantDto {
            id: id(request.id),
            product_id: request.product_id,
            external_id: request.external_id,
            sku: request.sku,
            title: request.title,
            option_values: request.option_values,
            price_cents: request.price_cents,
            compare_at_price_cents: request.compare_at_price_cents,
            requires_shipping: request.requires_shipping.unwrap_or(true),
            created_at: Some(now.clone()),
            updated_at: Some(now),
        };
        self.store.insert(Record::Variant(record)).await?.try_into()
    }
    async fn get_variant(&self, id: &str) -> BlueprintResult<VariantDto> {
        self.one(Selector::id(RecordKind::Variant, id)).await
    }
    async fn get_variant_by_external_id(&self, external_id: &str) -> BlueprintResult<VariantDto> {
        self.one(Selector::new(
            RecordKind::Variant,
            Field::ExternalId,
            external_id,
        ))
        .await
    }
    async fn update_variant(&self, request: UpdateVariantDto) -> BlueprintResult<VariantDto> {
        self.mutate(
            Selector::id(RecordKind::Variant, request.id.clone()),
            Mutation::PatchVariant(request),
        )
        .await
    }
    async fn delete_variant(&self, id: &str) -> BlueprintResult<()> {
        self.store
            .delete(Selector::id(RecordKind::Variant, id))
            .await
    }
    async fn list_variants(&self, ids: Option<Vec<String>>) -> BlueprintResult<Vec<VariantDto>> {
        self.many(Query {
            ids,
            ..Query::all(RecordKind::Variant)
        })
        .await
    }
    async fn list_variants_by_product(&self, product_id: &str) -> BlueprintResult<Vec<VariantDto>> {
        self.many(Query {
            equals: Some((Field::ProductId, product_id.into())),
            ..Query::all(RecordKind::Variant)
        })
        .await
    }
}
impl<S: EcommerceStore> InventoryService for BaseEcommerceService<S> {
    async fn create_inventory(&self, request: CreateInventoryDto) -> BlueprintResult<InventoryDto> {
        let now = timestamp();
        let record = InventoryDto {
            id: id(request.id),
            variant_id: request.variant_id,
            stock: request.stock,
            created_at: Some(now.clone()),
            updated_at: Some(now),
        };
        self.store
            .insert(Record::Inventory(record))
            .await?
            .try_into()
    }
    async fn get_inventory(&self, variant_id: &str) -> BlueprintResult<InventoryDto> {
        self.one(Selector::new(
            RecordKind::Inventory,
            Field::VariantId,
            variant_id,
        ))
        .await
    }
    async fn adjust_stock(&self, request: AdjustStockDto) -> BlueprintResult<InventoryDto> {
        self.mutate(
            Selector::new(RecordKind::Inventory, Field::VariantId, request.variant_id),
            Mutation::AdjustStock(request.delta),
        )
        .await
    }
    async fn check_stock(&self, request: StockCheckDto) -> BlueprintResult<StockCheckResultDto> {
        let record = self.get_inventory(&request.variant_id).await?;
        Ok(StockCheckResultDto {
            variant_id: request.variant_id,
            available: record.stock >= request.requested,
            stock: record.stock,
        })
    }
}
