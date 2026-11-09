// Compile the actual generated billing library template and export both profiles.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const temporary=mkdtempSync(path.join(tmpdir(),'fl-billing-date-artifacts-'));
const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
const template=readFileSync(path.join(root,'cli/src/templates/rust/billing/api/contracts/registry.rs'),'utf8');
function run(command,args,env={}) {
 const result=spawnSync(command,args,{cwd:temporary,env:{...process.env,CARGO_TARGET_DIR:target,CARGO_INCREMENTAL:'0',...env},encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024});
 assert.equal(result.status,0,result.stderr);return result.stdout;
}
try {
 mkdirSync(path.join(temporary,'src'));
 const crates={
  'forklaunch-core':'framework-rs/crates/core',
  'forklaunch-core':'framework-rs/crates/core',
  'forklaunch-billing-interface':'blueprint-rs/crates/interfaces/billing-interface',
  'forklaunch-billing-stripe':'blueprint-rs/crates/implementations/billing/billing-stripe'
 };
 const dependencies=Object.entries(crates).map(([name,relative])=>`${name} = { path = ${JSON.stringify(path.join(root,relative))}${name==='forklaunch-core'?', features = ["openapi"]':name==='forklaunch-billing-stripe'?', features = ["bound-checkout"]':''} }`).join('\n');
 writeFileSync(path.join(temporary,'Cargo.toml'),`[package]\nname="fl-billing-date-artifacts"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nserde_json="1"\n${dependencies}`);
 writeFileSync(path.join(temporary,'src/lib.rs'),'mod extensions;\nmod registry;\npub use registry::*;\n');
 writeFileSync(path.join(temporary,'src/extensions.rs'),'pub fn contracts() -> Vec<forklaunch_core::contract::RouteContract> { vec![] }');
 writeFileSync(path.join(temporary,'src/main.rs'),`fn main() {
  let contract=std::path::PathBuf::from(std::env::var("ARTIFACT_DIR").unwrap()).join("contract.json");
  let openapi=contract.with_file_name("openapi.json");
  let result=fl_billing_date_artifacts::write_contract_artifacts(&contract,&openapi);
  if std::env::var("BILLING_REQUEST_DATE_PROFILE").as_deref()==Ok("invalid") {
    assert!(result.is_err()); assert!(!contract.exists()); assert!(!openapi.exists());
  } else { result.unwrap(); }
 }`);
 for(const stripe of [false,true]) {
  const rendered=template.replace(/{{#stripe}}([\s\S]*?){{\/stripe}}/g,(_,body)=>stripe?body:'').replace(/{{\^stripe}}([\s\S]*?){{\/stripe}}/g,(_,body)=>stripe?'':body).replaceAll('{{{name}}}','billing-date-artifacts');
  assert.ok(!rendered.includes('{{'));writeFileSync(path.join(temporary,'src/registry.rs'),rendered);
  run('cargo',['build','--quiet','--manifest-path',path.join(temporary,'Cargo.toml')]);
  const results=[];
  for(const profile of ['legacy','utc-iso-and-epoch','invalid']) {
   const directory=path.join(temporary,`${stripe}-${profile}`);
   run(path.join(target,'debug/fl-billing-date-artifacts'),[],{ARTIFACT_DIR:directory,BILLING_REQUEST_DATE_PROFILE:profile});
   if(profile==='invalid')continue;
   const contract=JSON.parse(readFileSync(path.join(directory,'contract.json')));
   const openapi=JSON.parse(readFileSync(path.join(directory,'openapi.json')));
   results.push(contract);
   const text=JSON.stringify({contract,openapi});
   assert.equal(text.includes('x-forklaunch-date-profile'),profile!=='legacy');
  }
  assert.deepEqual(results[0].routes.map(r=>r.responses),results[1].routes.map(r=>r.responses));
 }
 console.log('PASS generated billing base/Stripe library artifact export: legacy, opt-in date metadata, unchanged responses, invalid-profile zero writes');
} finally {rmSync(temporary,{recursive:true,force:true});}
