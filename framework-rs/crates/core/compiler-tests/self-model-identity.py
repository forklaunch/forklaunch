#!/usr/bin/env python3
"""Compile and exercise recursive Self DTOs through both emitted SDK languages."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="self-model-proof-") as temporary:
    root = Path(temporary)
    env = dict(os.environ, CARGO_INCREMENTAL="0", FORKLAUNCH_SELF_MODEL_DIR=str(root))
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(repo / "framework-rs/Cargo.toml"), "-p", "forklaunch-core", "--features", "build,typed-schema", "--test", "self_model_identity"], env=env, check=True)
    (root / "node_modules").symlink_to(Path(os.environ["FORKLAUNCH_SDK_NODE_MODULES"]), target_is_directory=True)
    (root / "package.json").write_text('{"type":"module"}')
    (root / "consumer.ts").write_text('''import {createForkLaunchSdk, type Node, type GenericNodeForString, type Envelope} from './client.js';
const root:Node={value:'parent',child:{value:'child',child:null}};
const generic:GenericNodeForString={value:'parent',children:[{value:'child',children:[]}]};
const value:Envelope={root,generic};
const sdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(value),{headers:{'content-type':'application/json'}})});
const response=await sdk.recursive.envelope.envelope();if(response.status===200){const typed:Node=response.response.root;if(typed.child?.value!=='child')throw Error('recursive identity lost');}
const invalid=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify({...value,root:{value:'parent',child:{value:42,child:null}}}),{headers:{'content-type':'application/json'}})});
let rejected=false;try{await invalid.recursive.envelope.envelope();}catch{rejected=true;}if(!rejected)throw Error('recursive validation lost');
''')
    subprocess.run(["node", os.environ["FORKLAUNCH_TYPESCRIPT_COMPILER"], "--strict", "--target", "ES2022", "--module", "NodeNext", "--lib", "ES2022,DOM,DOM.Iterable", "client.ts", "consumer.ts"], cwd=root, check=True)
    subprocess.run(["node", "consumer.js"], cwd=root, check=True)
    (root / "Cargo.toml").write_text('''[package]
name="self-model-identity-proof"
version="0.0.0"
edition="2024"
[workspace]
[lib]
path="client.rs"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="=1.0.151"
reqwest={version="0.13",default-features=false,features=["json","query","stream","multipart","rustls"]}
forklaunch-universal-sdk={path='''+json.dumps(str(repo/'framework-rs/crates/universal-sdk'))+'''}
''')
    with (root / "client.rs").open("a") as output:
        output.write('''\n#[cfg(test)]mod self_consumer {use super::*;
#[test]fn recursive_types_are_reused(){
let envelope:Envelope=serde_json::from_value(serde_json::json!({"root":{"value":"parent","child":{"value":"child","child":null}},"generic":{"value":"parent","children":[{"value":"child","children":[]}]}})).unwrap();
let root:NodeResponse=*envelope.root;let canonical:Node=root;assert_eq!(canonical.child.unwrap().unwrap().value,"child");let generic:GenericResponse=*envelope.generic;let canonical:GenericNodeForString=generic;assert_eq!(canonical.children[0].value,"child");
}}
''')
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(root / "Cargo.toml"), "--offline"], env=env, check=True)
print("PASS recursive Self identity: typed TS runtime and Rust consumer")
