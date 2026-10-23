#!/usr/bin/env python3
"""Run under worker_runtime_lock.py; compile the actual authoring and both DTO outputs."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="rust-patch-parity-") as directory:
    root = Path(directory)
    proof = root / "proof.json"
    env = dict(os.environ, CARGO_INCREMENTAL="0", FORKLAUNCH_PATCH_FIXTURE=str(proof), FORKLAUNCH_PATCH_SDK_DIR=str(root))
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(repo / "framework-rs/Cargo.toml"), "-p", "forklaunch-core", "--features", "build,typed-schema", "--test", "patch_schema"], env=env, check=True)
    modules = Path(os.environ["FORKLAUNCH_SDK_NODE_MODULES"])
    compiler = Path(os.environ["FORKLAUNCH_TYPESCRIPT_COMPILER"])
    (root / "package.json").write_text('{"type":"module"}')
    (root / "consumer.ts").write_text('''import type { WireUpdate } from './models.js';
const keep: WireUpdate = {expectedRevision: 1};
const clear: WireUpdate = {expectedRevision: 1, excerpt: null, profile: null};
const set: WireUpdate = {expectedRevision: 1, excerpt: 'hi', profile: {label: 'ok', alias: null}};
// @ts-expect-error a present patch string cannot be a number
const bad: WireUpdate = {expectedRevision: 1, excerpt: 1};
// @ts-expect-error required revision is not nullable
const revision: WireUpdate = {expectedRevision: null};
// @ts-expect-error nested present patch string cannot be boolean
const nested: WireUpdate = {expectedRevision: 1, profile: {label: 'ok', alias: false}};
void [keep, clear, set, bad, revision, nested];
''')
    subprocess.run(["node", str(compiler), "--strict", "--noEmit", "--target", "ES2022", "--module", "NodeNext", "consumer.ts"], cwd=root, check=True)
    oracle = '''const {createRequire}=require('node:module'); const {readFileSync}=require('node:fs');
const dependencies=createRequire(process.argv[1]+'/package.json'); const {z}=dependencies('zod');
const proof=JSON.parse(readFileSync(process.argv[2], 'utf8'));
const schema=z.object({expectedRevision:z.number().int().min(0).max(4294967295),excerpt:z.string().nullable().optional(),score:z.number().int().min(0).max(4294967295).nullable().optional(),profile:z.object({label:z.string().min(1).max(4),alias:z.string().nullable().optional()}).strict().nullable().optional()}).strict();
for(const {input,accepted} of proof.cases){const result=schema.safeParse(input); if(result.success!==accepted) throw Error('Zod/Rust patch acceptance differs'); if(accepted && JSON.stringify(result.data)!==JSON.stringify(input)) {require('node:assert/strict').deepEqual(result.data,input);}}
console.log(`PASS actual Zod and Rust patch roundtrips: ${proof.cases.length} cases`);
'''
    subprocess.run(["node", "-e", oracle, str(modules), str(proof)], check=True)
    (root / "Cargo.toml").write_text('''[package]
name="patch-generated-model-proof"
version="0.0.0"
edition="2024"
[workspace]
[lib]
path="models.rs"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="=1.0.151"
forklaunch-core={path=''' + json.dumps(str(repo / "framework-rs/crates/core")) + ''',features=["schema-validation"]}
''')
    with (root / "models.rs").open("a") as handle:
        handle.write('''\n#[cfg(test)] mod generated_patch_tests { use super::Update; #[test] fn preserves_all_states_and_checks_remote_shapes() {
let proof:serde_json::Value=serde_json::from_str(include_str!("proof.json")).unwrap();
let validator=forklaunch_core::contract::schema_validation::compile_checked(&proof["schema"]).unwrap();
for case in proof["cases"].as_array().unwrap() {
let value=case["input"].clone(); let parsed=serde_json::from_value::<Update>(value.clone());
let valid=validator(&value).is_ok() && parsed.is_ok(); assert_eq!(valid,case["accepted"].as_bool().unwrap());
if valid {assert_eq!(serde_json::to_value(parsed.unwrap()).unwrap(),value);}
}
let keep:Update=serde_json::from_str(r#"{"expectedRevision":1}"#).unwrap();assert!(keep.excerpt.is_none());
let clear:Update=serde_json::from_str(r#"{"expectedRevision":1,"excerpt":null}"#).unwrap();assert_eq!(clear.excerpt,Some(None));
let set:Update=serde_json::from_str(r#"{"expectedRevision":1,"excerpt":"hello"}"#).unwrap();assert_eq!(set.excerpt,Some(Some("hello".into())));
}}\n''')
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(root / "Cargo.toml")], env=env, check=True)
