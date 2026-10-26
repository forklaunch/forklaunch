use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    {{crate_module}}::bootstrapper::run().await
}
