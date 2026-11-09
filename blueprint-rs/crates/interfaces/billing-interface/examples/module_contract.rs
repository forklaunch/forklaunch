fn main() {
    let mut registry = forklaunch_billing_interface::module_contracts::registry(
        std::env::args().any(|arg| arg == "--stripe"),
    );
    for route in forklaunch_billing_interface::module_contracts::compliance_contracts() {
        registry.register(route);
    }
    if std::env::args().any(|arg| arg == "--stripe") {
        for route in forklaunch_billing_interface::module_contracts::callback_contracts() {
            registry.register(route);
        }
    }
    println!("{}", registry.into_artifact().to_json());
}
