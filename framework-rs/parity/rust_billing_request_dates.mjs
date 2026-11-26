// Actual configured date fields, deterministic profile and explicit unsupported boundary.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync,execFileSync} from 'node:child_process';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');let actual;actual=actualValidator(root,s=>s==='@forklaunch/validator/zod'?actual.statics:undefined);
const cases=[0,1.9,-1.9,8640000000000000,-8640000000000000,8640000000000001,null,true,false,{},[],undefined,'2026-10-14','2026-10-14T01:02:03.123456Z','2026-10-14T01:02:03','10/14/2026','invalid','+275760-09-13T00:00:00.000Z',-62167219200000,253402300799999];
const fields=[];
for(const variant of ['base','stripe'])for(const family of ['billingPortal','subscription','checkoutSession']){
 const source=actual.load(path.join(root,`blueprint/implementations/billing/${variant}/domain/schemas/zod/${family}.schema.ts`));
 for(const form of ['Create','Update']){let schema=source[form+family[0].toUpperCase()+family.slice(1)+'Schema'];if(typeof schema==='function'){if(variant==='base'&&form==='Update')schema=schema({uuidId:true});if(typeof schema==='function')schema=variant==='stripe'?schema({PENDING:'PENDING'}):schema({card:'card'},{USD:'USD'},{PENDING:'PENDING'});}
 for(const field of ['startDate','endDate','expiresAt'])if(schema?.[field])fields.push({name:`${variant}:${form}${family}.${field}`,field,validator:schema[field]});
 }
}
const requests=[],expected=[];
for(const {name,field,validator}of fields)for(const value of cases){const result=actual.validator.parse(validator,value),optional=actual.validator.parse(validator,undefined).ok;
 const body={providerFields:{expiresAt:value},stripeFields:{startDate:value}};if(value!==undefined)body[field]=value;
 requests.push({schema:{type:'object',properties:{[field]:{type:'string',format:'date-time'},providerFields:{},stripeFields:{}},required:optional?[]:[field]},body});expected.push({name,field,value,result,optional});}
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--features','http','--example','request_dates'],{cwd:root,stdio:'inherit'});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
const run=spawnSync(path.join(metadata.target_directory,'debug/examples/request_dates'),[],{input:requests.map(v=>JSON.stringify(v)).join('\n')+'\n',encoding:'utf8',maxBuffer:20*1024*1024});assert.equal(run.status,0,run.stderr);
const rows=run.stdout.trim().split('\n').map(JSON.parse);assert.equal(rows.length,expected.length);let accepted=0,invalid=0,unsupported=0;
for(let i=0;i<rows.length;i++){const row=rows[i],{name,field,value,result,optional}=expected[i];assert.deepEqual(row.value.providerFields,JSON.parse(JSON.stringify(requests[i].body.providerFields)));assert.deepEqual(row.value.stripeFields,JSON.parse(JSON.stringify(requests[i].body.stripeFields)));assert.deepEqual(row.schema.required,requests[i].schema.required);assert.equal(row.schema.properties[field]['x-forklaunch-date-profile'].name,'utc-iso-and-epoch');
 if(value===undefined){assert.equal(row.ok,true);assert.equal(Object.hasOwn(row.value,field),false);assert.equal(result.ok,optional);continue;}
 if(row.ok){assert.equal(result.ok,true,name);assert.equal(row.value[field],result.value.toISOString(),name);accepted++;}
 else if(row.code==='billing_validation'){assert.equal(result.ok,false,name);invalid++;}
 else{assert.equal(row.code,'billing_date_profile_unsupported');unsupported++;}
}
for(const value of [NaN,Infinity,-Infinity])assert.equal(actual.validator.parse(actual.statics.date,value).ok,false); // Nonfinite values are not JSON numbers.
assert.equal(fields.length,16);assert.equal(rows.length,320);
console.log(`PASS320 actual configured date-field/native comparisons: ${accepted} normalized, ${invalid} actual-invalid, ${unsupported} explicitly unsupported,16 omissions preserved; opaque fields unchanged. Nonfinite JS numbers rejected (not JSON).`);
