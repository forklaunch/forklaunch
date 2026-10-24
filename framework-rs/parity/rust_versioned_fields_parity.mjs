import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const external=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=external('zod/v3');
const {load,validator}=actualValidator(root,(specifier,file,load)=>
  file.endsWith('routerSharedLogic.ts') && /middleware|telemetry/.test(specifier)?{}:
  specifier==='@forklaunch/common'?{
    ...load(path.join(root,'framework/common/src/deepCloneWithoutUndefined.ts')),
    ...load(path.join(root,'framework/common/src/guards/isRecord.ts')),
  }:undefined);
const {parse}=load(path.join(root,'framework/core/src/http/middleware/request/parse.middleware.ts'));
const cases=[];
for(const id of [' a ','',' deny ',undefined])
for(const count of [undefined,'0','9','10','13','42','-1','oops'])
for(const header of [undefined,'0','10','11','12','13','14','15','42','66'])
  cases.push({params:{...(id===undefined?{}:{id}),extra:'stripped'},
    query:{...(count===undefined?{}:{count}),extra:'stripped'},
    headers:{...(header===undefined?{}:{'x-count':header}),extra:'retained'}});

function number(last,header){
  return z.preprocess(value=>value===undefined?(last?10:0):Number(value.trim()),
    z.number().int().min(last?10:0)).transform((number,context)=>{
    if((header&&number===66)||(!header&&last&&number===13)){
      context.addIssue({code:z.ZodIssueCode.custom,message:'private transform error'});return z.NEVER;
    }
    if(number===42)return undefined;
    if(!header)return number===0?null:number+(last?100:0);
    return new Map([[0,0],[10,null],[11,false],[12,''],[13,[]],[14,{}]]).has(number)?
      new Map([[0,0],[10,null],[11,false],[12,''],[13,[]],[14,{}]]).get(number):number*2;
  });
}
const schemas=Object.fromEntries(['first','last'].map(version=>[version,z.object({
  params:z.object({id:z.string().trim().min(1).transform((value,context)=>{
    if(value==='deny'){context.addIssue({code:z.ZodIssueCode.custom,message:'private parameter error'});return z.NEVER;}
    return value.toUpperCase();
  })}),
  query:z.object({count:number(version==='last',false)}),
  headers:z.object({'x-count':number(version==='last',true)}),
  body:z.unknown(),
})]));
const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',
  path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','version_fields'],
  {input:JSON.stringify(cases),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'},timeout:120000}));
assert.equal(native.length,cases.length);
for(const [index,input] of cases.entries()){
  const req={...structuredClone(input),context:{correlationId:'fields'},
    contractDetails:{versions:schemas},requestSchema:schemas,schemaValidator:validator};
  let status=200,next=false;
  const res={type(){return this;},status(value){status=value;return this;},send(){return this;}};
  parse(req,res,()=>{next=true;});
  assert.equal(next,status!==400);
  const expected=status===400?{status}:{status,version:req.version,matched:Array.from(req._parsedVersions),
    params:req.params,query:req.query,headers:req.headers};
  assert.deepEqual(native[index],JSON.parse(JSON.stringify(expected)),`field case ${index}: ${JSON.stringify(input)}`);
}
console.log(`PASS ${cases.length} actual TypeScript/native field defaults, transforms, absence, header truthiness, stripping, overlap and rejection comparisons`);
