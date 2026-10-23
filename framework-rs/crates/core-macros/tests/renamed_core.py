#!/usr/bin/env python3
"""Compile schema macros through an actual renamed core dependency, as generated Notes does."""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

framework = Path(__file__).resolve().parents[3]
repo = framework.parent
with tempfile.TemporaryDirectory(prefix="core-contract-alias-") as directory:
    root = Path(directory)
    (root / "Cargo.toml").write_text(f'''[package]
name="core-contract-alias-proof"
version="0.0.0"
edition="2024"
[workspace]
[lib]
path="lib.rs"
[dependencies]
framework_core={{package="forklaunch-core",path="{framework / 'crates/core'}",features=["typed-schema","schema-validation"]}}
serde_json="=1.0.151"
''')
    (root / "lib.rs").write_text('''#[framework_core::contract::schema(rename_all="camelCase", unknown_fields="deny")]
pub struct Note {
    #[field(min_length=2,max_length=10)]
    pub body_text:String,
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn aliased_macro_runtime_schema_and_wire_contract_agree() {
        let schema=framework_core::contract::schema_value_for::<Note>();
        let validate=framework_core::contract::schema_validation::compile_checked(&schema).unwrap();
        let valid=serde_json::json!({"bodyText":"hi"});
        assert!(validate(&valid).is_ok());
        let note:Note=serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(note).unwrap(),valid);
        for invalid in [serde_json::json!({"bodyText":"x"}),serde_json::json!({"bodyText":"hi","extra":true})] {
            assert!(validate(&invalid).is_err());
            assert!(serde_json::from_value::<Note>(invalid).is_err());
        }
        assert!(serde_json::to_value(Note{body_text:"x".into()}).is_err());
    }
}
''')
    environment=dict(os.environ,CARGO_INCREMENTAL="0",CARGO_TARGET_DIR=str(repo / "cli/target"))
    command="cargo +stable test --quiet --manifest-path " + shlex.quote(str(root / "Cargo.toml"))
    subprocess.run(["python3",str(repo / "cli/tests/fixtures/worker_runtime_lock.py"),"-c",command],env=environment,check=True)
