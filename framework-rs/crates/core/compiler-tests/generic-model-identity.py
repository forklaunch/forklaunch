#!/usr/bin/env python3
"""Compile and exercise concrete generic DTOs through both emitted SDK languages."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="generic-model-proof-") as temporary:
    root = Path(temporary)
    env = dict(os.environ, CARGO_INCREMENTAL="0", FORKLAUNCH_GENERIC_MODEL_DIR=str(root))
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(repo / "framework-rs/Cargo.toml"), "-p", "forklaunch-core", "--features", "build,typed-schema", "--test", "generic_model_identity"], env=env, check=True)
    (root / "node_modules").symlink_to(Path(os.environ["FORKLAUNCH_SDK_NODE_MODULES"]), target_is_directory=True)
    (root / "package.json").write_text('{"type":"module"}')
    (root / "consumer.ts").write_text('''import {createForkLaunchSdk, type PageForPerson, type PageForString, type Envelope} from './client.js';
const page:PageForPerson={items:[{name:'Ada'}]};
const strings:PageForString={items:['label']};
const value:Envelope={page,strings,pair:{first:{name:'Ada'},second:'value'},first:{value:'v1'},second:{value:'v2'}};
const sdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(value),{headers:{'content-type':'application/json'}})});
const response=await sdk.generic.envelope.envelope();if(response.status===200){const typed:PageForPerson=response.response.page;if(typed.items[0].name!=='Ada')throw Error('generic nested type lost');}
const invalid=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify({...value,page:{items:['wrong']}}),{headers:{'content-type':'application/json'}})});
let rejected=false;try{await invalid.generic.envelope.envelope();}catch{rejected=true;}if(!rejected)throw Error('generic nested validation lost');
''')
    subprocess.run(["node", os.environ["FORKLAUNCH_TYPESCRIPT_COMPILER"], "--strict", "--target", "ES2022", "--module", "NodeNext", "--lib", "ES2022,DOM,DOM.Iterable", "client.ts", "consumer.ts"], cwd=root, check=True)
    subprocess.run(["node", "consumer.js"], cwd=root, check=True)
    (root / "Cargo.toml").write_text('''[package]
name="generic-model-identity-proof"
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
        output.write('''\n#[cfg(test)]mod generic_consumer {use super::*;
#[test]fn instantiated_types_are_reused(){
let envelope:Envelope=serde_json::from_value(serde_json::json!({"page":{"items":[{"name":"Ada"}]},"strings":{"items":["label"]},"pair":{"first":{"name":"Ada"},"second":"value"},"first":{"value":"v1"},"second":{"value":"v2"}})).unwrap();
let page:PeopleResponse=envelope.page;let canonical:PageForPerson=page;assert_eq!(canonical.items[0].name,"Ada");let strings:PageForString=envelope.strings;assert_eq!(strings.items[0],"label");
}}
''')
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(root / "Cargo.toml"), "--offline"], env=env, check=True)
print("PASS generic model identity: typed TS runtime and Rust consumer")
