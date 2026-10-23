#!/usr/bin/env python3
"""Prove SDK bytes do not depend on serde_json preserve_order feature unification."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="compiler-order-proof-") as temporary:
    root = Path(temporary)
    (root / 'src').mkdir()
    (root / 'Cargo.toml').write_text('''[package]
name="compiler-order-proof"
version="0.0.0"
edition="2024"
[workspace]
[features]
ordered=["serde_json/preserve_order"]
[dependencies]
forklaunch-core={path='''+json.dumps(str(repo/'framework-rs/crates/core'))+''',features=["build"]}
serde_json="=1.0.151"
''')
    (root/'src/main.rs').write_text('''use forklaunch_core::{contract::*,codegen::{SdkTarget,SdkGenerationPlan}};
fn main(){
let schema=r#"{"type":"object","required":["z","a"],"properties":{"z":{"type":"array","items":{"type":"string"}},"a":{"type":"object","properties":{"last":{"type":"string"},"first":{"type":"boolean"}},"required":["last","first"]}},"additionalProperties":false}"#;
let mut body=BodyContract::json("Record");body.schema=SchemaRef::named("Record").with_json_schema(schema);
let mut registry=ContractRegistry::for_service(ServiceContract::new("ordered","1"));
registry.register(RouteContract::new("create","Create",HttpMethod::Post,"/items").with_request_body(body.clone()).with_response(200,Some(body)).with_response_header(200,FieldContract::required("x-z","String")).with_response_header(200,FieldContract::required("x-a","String")));
let native=SdkGenerationPlan::from_registry(SdkTarget::TypeScript,&registry);
let source=forklaunch_core::typescript::generate_typescript_http_sdk_source(&native,&forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("ordered")).unwrap();
let compiled=forklaunch_core::build::http_artifact_sources(&registry.into_artifact().to_json(),"ordered").unwrap();assert_eq!(source,compiled.typescript);
let output=std::env::args().nth(1).unwrap();std::fs::write(format!("{output}.ts"),compiled.typescript).unwrap();std::fs::write(format!("{output}.rs"),compiled.rust).unwrap();
}
''')
    env=dict(os.environ,CARGO_INCREMENTAL='0')
    for name, features in [('sorted',[]),('ordered',['--features','ordered'])]:
        subprocess.run(['cargo','+stable','run','--manifest-path',str(root/'Cargo.toml'),'--offline',*features,'--',str(root/name)],check=True,env=env)
    for suffix in ['ts','rs']:
        assert (root/f'sorted.{suffix}').read_bytes()==(root/f'ordered.{suffix}').read_bytes(), f'{suffix} output depends on serde_json features'
print('PASS complete TypeScript/Rust bytes with and without serde_json preserve_order')
