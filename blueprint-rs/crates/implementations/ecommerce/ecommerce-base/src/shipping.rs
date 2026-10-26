//! The module's domestic/international table rate with a free-shipping threshold.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::pricing::{
    ShippingCalculation, ShippingCalculationResult, ShippingService,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct FlatRateShippingService;
impl ShippingService for FlatRateShippingService {
    async fn calculate(
        &self,
        request: ShippingCalculation,
    ) -> BlueprintResult<ShippingCalculationResult> {
        if request.subtotal_cents < 0 {
            return Err(BlueprintError::new(
                "ecommerce_validation",
                "Shipping subtotal must be nonnegative",
            ));
        }
        Ok(ShippingCalculationResult {
            shipping_cents: if request.subtotal_cents >= 5000 {
                0
            } else if request.shipping_address.country.eq_ignore_ascii_case("US") {
                599
            } else {
                1999
            },
        })
    }
}
