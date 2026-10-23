//! Checkout orchestration. Prices come from variants and provider credentials remain response-only.
use super::{EcommerceBackend, response};
use crate::{StripePaymentService, tax::StripeTaxService};
use axum::{Extension, Json, response::Response};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_base::shipping::FlatRateShippingService;
use forklaunch_ecommerce_interface::{
    CartService, CreateOrderDto, InventoryService, OrderItemDto, OrderStatus, PaymentDto,
    StockCheckDto, TransitionOrderDto, VariantService,
    module::{Checkout, CheckoutResult, CreateProviderPayment, CreatedPayment},
    pricing::{ShippingCalculation, ShippingService, TaxCalculation, TaxService},
};
use std::sync::Arc;

fn invalid(message: &str) -> BlueprintError {
    BlueprintError::new("ecommerce_validation", message)
}
impl EcommerceBackend {
    async fn resume_payment(
        &self,
        session: &AuthenticatedSession,
        payment: PaymentDto,
    ) -> BlueprintResult<CreatedPayment> {
        if payment
            .provider_ref
            .as_ref()
            .is_some_and(|reference| reference.starts_with("pi_"))
        {
            let mut provider =
                StripePaymentService::new(self.service(session)?, self.stripe.clone());
            if let Some(connect) = &self.connect {
                provider = provider.with_connect(connect.clone());
            }
            let resumed = provider.resume_payment_for_order(&payment.id).await?;
            Ok(CreatedPayment {
                payment: resumed.payment,
                client_secret: resumed.client_secret,
            })
        } else {
            Ok(CreatedPayment {
                payment,
                client_secret: None,
            })
        }
    }
    async fn price_order(
        &self,
        service: &forklaunch_ecommerce_base::BaseEcommerceService<
            forklaunch_ecommerce_base::postgres::PostgresEcommerceStore,
        >,
        cart: forklaunch_ecommerce_interface::CartDto,
        input: &Checkout,
    ) -> BlueprintResult<forklaunch_ecommerce_interface::OrderDto> {
        let mut items = Vec::new();
        let mut subtotal = 0_i64;
        for item in &cart.items {
            if item.quantity <= 0 {
                return Err(invalid("Cart quantities must be positive"));
            }
            if !service
                .check_stock(StockCheckDto {
                    variant_id: item.variant_id.clone(),
                    requested: item.quantity,
                })
                .await?
                .available
            {
                return Err(invalid("Insufficient stock for checkout"));
            }
            let variant = service.get_variant(&item.variant_id).await?;
            if variant.price_cents < 0 {
                return Err(invalid("Variant prices must be nonnegative"));
            }
            let amount = item
                .quantity
                .checked_mul(variant.price_cents)
                .ok_or_else(|| invalid("Checkout amount overflow"))?;
            subtotal = subtotal
                .checked_add(amount)
                .ok_or_else(|| invalid("Checkout amount overflow"))?;
            items.push(OrderItemDto {
                variant_id: item.variant_id.clone(),
                quantity: item.quantity,
                unit_price_cents: variant.price_cents,
            });
        }
        let tax = StripeTaxService::new(self.stripe.clone())
            .calculate(TaxCalculation {
                line_item_cents: vec![subtotal],
                shipping_address: input.shipping_address.clone(),
            })
            .await?;
        let shipping = FlatRateShippingService
            .calculate(ShippingCalculation {
                subtotal_cents: subtotal,
                shipping_address: input.shipping_address.clone(),
            })
            .await?;
        let total = subtotal
            .checked_add(tax.tax_cents)
            .and_then(|amount| amount.checked_add(shipping.shipping_cents))
            .ok_or_else(|| invalid("Checkout amount overflow"))?;
        service
            .create_order_for_cart(
                CreateOrderDto {
                    id: None,
                    customer_id: cart.customer_id,
                    items,
                    shipping_address: input.shipping_address.clone(),
                    subtotal_cents: subtotal,
                    discount_cents: 0,
                    tax_cents: tax.tax_cents,
                    tax_breakdown: tax.breakdown,
                    shipping_cents: shipping.shipping_cents,
                    gift_card_cents: 0,
                    total_cents: total,
                },
                Some(cart.id),
            )
            .await
    }
    pub(super) async fn checkout(
        &self,
        session: &AuthenticatedSession,
        input: Checkout,
    ) -> BlueprintResult<CheckoutResult> {
        let service = self.service(session)?;
        // Observe the pending order before admission: a concurrently created order
        // is the unique-insert race winner, rather than a stale basket replacement.
        let observed = service.pending_order_for_cart(&input.cart_id).await?;
        let _slot = self
            .orchestration_slots
            .acquire()
            .await
            .map_err(|_| invalid("Checkout unavailable"))?;
        let _guard = self
            .store
            .lock_operation("checkout-cart", &input.cart_id)
            .await?;
        let cart = service.get_cart(&input.cart_id).await?;
        let pending = service.pending_order_for_cart(&cart.id).await?;
        if cart.items.is_empty() && pending.is_none() {
            return Err(invalid("Cannot checkout an empty cart"));
        }
        let order = if let Some(order) = pending
            .as_ref()
            .filter(|_| cart.items.is_empty() || observed.is_none())
        {
            order.clone()
        } else {
            if let Some(stale) = pending {
                service
                    .store()
                    .transition_order_with_event(
                        TransitionOrderDto {
                            id: stale.id,
                            to: OrderStatus::Cancelled,
                        },
                        &self.queue,
                        "order-events",
                    )
                    .await?;
            }
            self.price_order(&service, cart, &input).await?
        };
        let payment_result = async {
            if let Some(payment) = service.pending_payment_for_order(&order.id).await? {
                self.resume_payment(session, payment).await
            } else {
                match self
                    .create_payment(
                        session,
                        CreateProviderPayment {
                            order_id: order.id.clone(),
                            amount_cents: order.total_cents,
                            currency: "usd".to_owned(),
                            provider: input.provider,
                        },
                    )
                    .await
                {
                    Ok(payment) => Ok(payment),
                    Err(error) if error.code == "ecommerce_conflict" => {
                        let winner = service
                            .pending_payment_for_order(&order.id)
                            .await?
                            .ok_or(error)?;
                        self.resume_payment(session, winner).await
                    }
                    Err(error) => Err(error),
                }
            }
        }
        .await;
        let created = payment_result.map_err(|_| {
            BlueprintError::new(
                "ecommerce_provider_unavailable",
                "Order created but payment could not be initiated; retry checkout",
            )
        })?;
        // Match the current TS controller: paid-webhook handling clears the cart.
        Ok(CheckoutResult {
            order,
            payment: created.payment,
            client_secret: created.client_secret,
        })
    }
}
pub(super) async fn checkout(
    Extension(backend): Extension<Arc<EcommerceBackend>>,
    Extension(session): Extension<AuthenticatedSession>,
    super::CheckedJson(Json(input)): super::CheckedJson<Checkout>,
) -> Response {
    response(backend.checkout(&session, input).await)
}
