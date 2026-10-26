//! Configuration probe for managed capability clients; no network calls.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
fn main() {
    match InstanceGatewayClient::from_env() {
        Ok(client) => println!(
            "{}",
            serde_json::json!({"ok":true,"description":format!("{client:?}")})
        ),
        Err(error) => println!(
            "{}",
            serde_json::json!({"error":true,"description":error.to_string()})
        ),
    }
}
