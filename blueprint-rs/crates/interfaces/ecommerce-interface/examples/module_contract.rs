//! Exports application contracts for conformance and SDK consumers.
fn main() {
    let mut registry = forklaunch_ecommerce_interface::contracts::registry();
    registry.register(forklaunch_ecommerce_interface::contracts::import_contract());
    registry.register(forklaunch_ecommerce_interface::contracts::checkout_contract());
    registry.register(forklaunch_ecommerce_interface::contracts::stripe_webhook_contract());
    registry.register(forklaunch_ecommerce_interface::contracts::paypal_webhook_contract());
    println!("{}", registry.into_artifact().to_json());
}
