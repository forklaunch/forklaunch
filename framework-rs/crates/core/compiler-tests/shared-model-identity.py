#!/usr/bin/env python3
"""Compile actual consumers of repeated authoritative model identities."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="shared-model-proof-") as temporary:
    root = Path(temporary)
    env = dict(os.environ, CARGO_INCREMENTAL="0", FORKLAUNCH_MODEL_IDENTITY_DIR=str(root), FORKLAUNCH_MODEL_IMPORT_DIR=str(root))
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(repo / "framework-rs/Cargo.toml"), "-p", "forklaunch-core", "--features", "build,typed-schema", "--test", "shared_model_identity"], env=env, check=True)
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(repo / "framework-rs/Cargo.toml"), "-p", "forklaunch-core", "--features", "build,typed-schema", "--lib", "build::imports::models::tests::imported_components_share_authoritative_identity_not_titles"], env=env, check=True)
    (root / "node_modules").symlink_to(Path(os.environ["FORKLAUNCH_SDK_NODE_MODULES"]), target_is_directory=True)
    (root / "package.json").write_text('{"type":"module"}')
    (root / "consumer.ts").write_text('''import {createForkLaunchSdk, type Person, type PersonEnvelope, type Label, type ResponseModel, type DateModel, type HeadersModel, type AjvModel} from './client.js';
import {createForkLaunchSdk as importedSdk, type Person as ImportedPerson} from './imported.js';
const defaultsSdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(_url,init)=>{if(init?.body!==JSON.stringify({settings:{}}))throw Error('request default was materialized');return new Response(JSON.stringify({settings:{count:0}}),{headers:{'content-type':'application/json'}});}});
const defaults=await defaultsSdk.identity.settings.settings({body:{settings:{}}});if(defaults.status===200 && defaults.response.settings.count!==0)throw Error('default output lost');
const invalidDefaults=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify({settings:{}}),{headers:{'content-type':'application/json'}})});let invalidDefault=false;try{await invalidDefaults.identity.settings.settings({body:{settings:{}}});}catch{invalidDefault=true;}if(!invalidDefault)throw Error('default output became optional');
const shared: Person = {name:'Ada'};
const nested:PersonEnvelope={person:shared,people:[shared],optional:null};
const nestedSdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(nested),{headers:{'content-type':'application/json'}})});
const wrapped=await nestedSdk.identity.envelope.envelope();if(wrapped.status===200){const member:Person=wrapped.response.person;if(member.name!=='Ada')throw Error('nested identity lost');}
const invalidNested=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify({...nested,person:{name:''}}),{headers:{'content-type':'application/json'}})});let rejected=false;try{await invalidNested.identity.envelope.envelope();}catch{rejected=true;}if(!rejected)throw Error('nested constraints lost');
const imported=importedSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(shared),{headers:{'content-type':'application/json'}})});
const importedResponse=await imported.people.person.readPerson();if(importedResponse.status===200){const value:ImportedPerson=importedResponse.response;if(value.name!=='Ada')throw Error('imported model lost');}
const status:ResponseModel={ok:true};if(!status.ok)throw Error('wrong response model');
const independent: Label = {name:'Label'};
const sdk = createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(url)=>String(url).endsWith('/people/events')?new Response('data: '+JSON.stringify(shared)+'\\n\\n',{headers:{'content-type':'text/event-stream'}}):new Response(JSON.stringify(String(url).endsWith('/dates')?{at:'2024-02-29T12:34:56.123Z'}:shared),{headers:{'content-type':'application/json'}})});
const created=await sdk.identity.people.create();
const read=await sdk.identity.people.read();
const events=await sdk.identity.people.watchPeople();if(events.status===200){for await(const event of events.response){const value:Person=event.data;if(value.name!=='Ada')throw Error('SSE identity lost');}}
const date=await sdk.identity.dates.date();if(date.status===200){const typed:DateModel=date.response;if(!(typed.at instanceof Date)||typed.at.toISOString()!=='2024-02-29T12:34:56.123Z')throw Error('Date runtime shadowed');}
const headers:HeadersModel={value:'h'};const validator:AjvModel={value:'a'};if(headers.value+validator.value!=='ha')throw Error('model lost');
if(created.code===200 && read.code===200){const a:Person=created.response; const b:Person=read.response;if(a.name!==b.name)throw Error('shared values differ');}
if(independent.name!=='Label')throw Error('distinct schema lost');
''')
    subprocess.run(["node", os.environ["FORKLAUNCH_TYPESCRIPT_COMPILER"], "--strict", "--target", "ES2022", "--module", "NodeNext", "--lib", "ES2022,DOM,DOM.Iterable", "client.ts", "consumer.ts"], cwd=root, check=True)
    subprocess.run(["node", "consumer.js"], cwd=root, check=True)
    (root / "Cargo.toml").write_text('''[package]
name="shared-model-identity-proof"
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
        output.write('''\n#[path="imported.rs"]mod imported;
#[cfg(test)]mod identity_consumer {use super::*;
#[test]fn operation_models_are_interchangeable_without_conversion(){
let imported:imported::ListPeopleResponse=imported::Person{name:"Ada".into()};let reused:imported::ReadPersonResponse=imported;assert_eq!(reused.name,"Ada");
let envelope:PersonEnvelope=serde_json::from_value(serde_json::json!({"person":{"name":"Ada"},"people":[{"name":"Bea"}],"optional":null})).unwrap();
let person:Person=envelope.person;let member:Person=envelope.people.into_iter().next().unwrap();assert_eq!(member.name,"Bea");
let page:PersonPage=serde_json::from_value(serde_json::json!({"selected":{"name":"Ada"},"entries":[]})).unwrap();let selected:Person=page.selected;assert_eq!(selected.name,person.name);
let value:CreateResponse=Person{name:"Ada".into()};
let reused:ReadResponse=value;
let canonical:Person=reused;
let event:WatchPeopleResponse=serde_json::from_value(serde_json::json!({"data":{"name":"Ada"}})).unwrap();let value:Person=event.data;assert_eq!(value.name,"Ada");
let event:WatchTreeResponse=serde_json::from_value(serde_json::json!({"data":{"value":"root","child":null}})).unwrap();let _:Tree=event.data;
assert_eq!(serde_json::to_value(canonical).unwrap(),serde_json::json!({"name":"Ada"}));
assert!(ResponseModel{ok:true}.ok);
let date:DateModel=serde_json::from_str(r#"{"at":"2024-02-29T12:34:56.123Z"}"#).unwrap();assert_eq!(date.at.timestamp_millis(),1_709_210_096_123);
let separate=Label{name:"Label".into()};assert_eq!(separate.name,"Label");
let tree:CreateTreeResponse=serde_json::from_value(serde_json::json!({"value":"root","child":{"value":"leaf","child":null}})).unwrap();
let tree:ReadTreeResponse=tree;assert_eq!(serde_json::to_value(tree).unwrap()["child"]["value"],"leaf");
}}
''')
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(root / "Cargo.toml")], env=env, check=True)
    print("PASS shared native model identity: generated TypeScript runtime and Rust cross-operation assignment")
