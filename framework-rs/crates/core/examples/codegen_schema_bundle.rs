//! Conformance bridge for the actual private schema graph compiler.
#[allow(dead_code)]
#[path = "../src/codegen/schema.rs"]
mod schema;
use std::io::{self, Read};
fn main() -> anyhow::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let inputs: Vec<serde_json::Value> = serde_json::from_str(&input)?;
    let output = inputs.iter().map(|entry| {
        let input = &entry["schema"];
        let bundled = if entry["runtime"] == true { schema::scoped_runtime_schema(input)? } else { schema::bundle(input, input, &mut schema::ExpansionBudget::default())? };
        let original = forklaunch_core::ws::codec::PayloadCodec::<serde_json::Value>::json_schema(input)?;
        let compiled = forklaunch_core::ws::codec::PayloadCodec::<serde_json::Value>::json_schema(&bundled)?;
        let cases = entry["cases"].as_array().unwrap();
        let original_results = cases.iter().map(|case| original.decode(&serde_json::to_vec(&case[0]).unwrap()).is_ok()).collect::<Vec<_>>();
        let compiled_results = cases.iter().map(|case| compiled.decode(&serde_json::to_vec(&case[0]).unwrap()).is_ok()).collect::<Vec<_>>();
        let models = forklaunch_core::codegen::compile_schema("Fixture", input)?;
        Ok(serde_json::json!({"schema":bundled,"original":original_results,"compiled":compiled_results,"typescript":models.typescript_type,"definitions":models.typescript_definitions}))
    }).collect::<anyhow::Result<Vec<_>>>()?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
