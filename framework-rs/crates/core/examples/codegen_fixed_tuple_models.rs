//! Emit fixed-tuple models for actual validator and generated Serde conformance.
use std::io::{self, Read};
fn main() -> anyhow::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let root_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "TupleCase".to_owned());
    let schemas: Vec<serde_json::Value> = serde_json::from_str(&input)?;
    let output = schemas.iter().map(|schema| {
        let models = forklaunch_core::codegen::compile_schema(&root_name, schema)?;
        let codec = forklaunch_core::ws::codec::PayloadCodec::<serde_json::Value>::json_schema(schema)?;
        if schema.get("$schema").is_none() && schema.get("prefixItems").is_some() {
            assert!(codec.decode(br#"["x"]"#).is_ok());
            assert!(codec.decode(b"[1]").is_err());
        }
        let source = forklaunch_core::codegen::render_rust_models(&models.rust_models, &models.rust_aliases)?;
        Ok(serde_json::json!({"source":source,"root":models.rust_type,"typescript":models.typescript_type,"definitions":models.typescript_definitions,"sequences":models.rust_models.iter().filter(|model|model.sequence).count()}))
    }).collect::<anyhow::Result<Vec<_>>>()?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
