// Compare native retention hooks with the actual TS service over explicit stores.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const ts=process.env.TSC_BIN?require(path.resolve(path.dirname(fs.realpathSync(process.env.TSC_BIN)),'../lib/typescript.js')):require('typescript');
assert.equal(typeof ts.transpileModule,'function','Use a TypeScript compiler with transpileModule');
process.env.TZ='UTC';
let tenant;
function load(file,imports){
 const exports={};
 const js=ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 class FixedDate extends Date {constructor(...args){super(...(args.length?args:[86410000]));}static now(){return 86410000;}}
 vm.runInNewContext(js,{exports,Date:FixedDate,require(name){assert.ok(name in imports,name);return imports[name];}});
 return exports;
}
const compliance=load('framework/core/src/persistence/complianceTypes.ts',{});
const {RetentionService}=load('framework/core/src/services/retentionService.ts',{
 '../persistence/complianceTypes':compliance,
 '../persistence/encryptedType':{async withEncryptionContext(id,callback){const previous=tenant;tenant=id;try{return await callback();}finally{tenant=previous;}}}
});
const rows=[
 {tenant:'a',created:9,marked:false,email:'private',diagnosis:'required',note:'public'},
 {tenant:'a',created:10,marked:false,email:'at-boundary',diagnosis:'required',note:'public'},
 {tenant:'a',created:2,marked:true,email:'already-marked',diagnosis:'required',note:'public'},
 {tenant:'b',created:1,marked:false,email:'other-tenant',diagnosis:'required',note:'public'},
 {tenant:'foreign',created:1,marked:false,email:'untouched',diagnosis:'required',note:'public'},
];
const cases=[];
for(const action of ['delete','anonymize'])for(const dryRun of [false,true])for(const batchSize of [1,2,1000])for(const tenants of [['a'],['a','b']])cases.push({action,dryRun,batchSize,tenants,rows});
const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--example','retention_hooks'],{cwd:root,encoding:'utf8',input:JSON.stringify(cases),timeout:180000}));
for(const [index,scenario]of cases.entries()){
 let stored=structuredClone(rows).map(row=>({...row,createdAt:new Date(row.created*1000),retentionAnonymizedAt:row.marked?new Date(0):null}));
 compliance.registerEntityCompliance('Contact',new Map([['email','pii'],['diagnosis','phi'],['note','none']]));
 compliance.registerEntityRetention('Contact',{duration:'P1D',action:scenario.action});
 const selected=filter=>stored.filter(row=>row.tenant===tenant&&row.createdAt<filter.createdAt.$lt&&(!('retentionAnonymizedAt'in filter)||row.retentionAnonymizedAt===null));
 const orm={getMetadata:()=>({getAll:()=>new Map([['Contact',{className:'Contact',properties:{email:{nullable:true},diagnosis:{nullable:false},note:{nullable:true}}}]])}),em:{fork:()=>({count:async(_,filter)=>selected(filter).length,find:async(_,filter,options)=>selected(filter).sort((a,b)=>a.createdAt-b.createdAt).slice(0,options.limit),remove(row){stored=stored.filter(candidate=>candidate!==row);},flush:async()=>{}})}};
 const service=new RetentionService(orm,{info(){},warn(){},error(){}});
 const result=await service.enforce({tenantIds:scenario.tenants,dryRun:scenario.dryRun,batchSize:scenario.batchSize});
 delete result.durationMs;
 result.rows=stored.map(({createdAt,retentionAnonymizedAt,...row})=>({...row,marked:retentionAnonymizedAt!==null}));
 assert.deepEqual(native[index],JSON.parse(JSON.stringify(result)),JSON.stringify({...scenario,rows:undefined}));
}
console.log(`PASS ${cases.length} actual TS/native retention cases: exclusive cutoffs, tenant scopes, dry runs, bounded batches, delete and classified nullable-field anonymization`);
