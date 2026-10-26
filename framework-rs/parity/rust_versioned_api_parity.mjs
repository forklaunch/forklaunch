// Exercise the repository's actual request parser/validator and native matcher.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const external=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=external('zod/v3');
const {load,validator}=actualValidator(root,(specifier,file,load)=>file.endsWith('routerSharedLogic.ts') && /middleware|telemetry/.test(specifier)?{}:specifier==='@forklaunch/common'?{
 ...load(path.join(root,'framework/common/src/deepCloneWithoutUndefined.ts')),
 ...load(path.join(root,'framework/common/src/guards/isRecord.ts')),
}:undefined);
const {parse}=load(path.join(root,'framework/core/src/http/middleware/request/parse.middleware.ts'));
const fixtures=[];
for(const names of [['z-first','a-last'],['2','1'],['10','2'],['01','1'],['4294967295','0']]) {
 const versions=names.map((name,index)=>({name,schema:{type:'object',required:['value'],properties:{value:{type:'string',minLength:index+1}}}}));
 fixtures.push({versions,cases:[{value:'a'},{value:'ab'},{value:''},{value:10},{},null,{value:'abc',extra:true}]});
}
fixtures.push({kind:'text',versions:[{name:'z-first',schema:{type:'string',minLength:0}},{name:'a-last',schema:{type:'string',minLength:2}}],cases:['','a','ab','水','水a']});
fixtures.push({kind:'file',versions:[{name:'z-first',schema:{type:'string',format:'binary',title:'File'}},{name:'a-last',schema:{type:'string',format:'binary'}}],cases:[[],[0,255,128],[97,98],[230,176,180],[34,123,125,34]]});
const parserFixtures=[['json','text'],['text','text'],['file','binary'],['none','json'],['none','none'],['urlEncoded','multipart']].map(parserKinds=>({parserKinds}));
const {validateContractDetails}=load(path.join(root,'framework/core/src/http/router/routerSharedLogic.ts'));
const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','version_selection'],{input:JSON.stringify([...fixtures,...parserFixtures]),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'}}));
let count=0;
for(const [i,fixture] of fixtures.entries()) {
 const requestSchema=Object.fromEntries(fixture.versions.map(version=>[version.name,z.object({body:fixture.kind==='file'?validator.file:fixture.kind==='text'?z.string().min(version.schema.minLength):z.object({value:z.string().min(version.schema.properties.value.minLength)}),params:z.object({}),query:z.object({}),headers:z.object({})})]));
 assert.deepEqual(native[i].order,Object.keys(requestSchema));
 for(const [j,body] of fixture.cases.entries()) {
  const req={body:fixture.kind==='file'?Buffer.from(body):body,params:{},query:{},headers:{},context:{correlationId:'fixture'},contractDetails:{versions:requestSchema},requestSchema,schemaValidator:validator};
  let status=200,next=false;
  const res={type(){return this;},status(code){status=code;return this;},send(){return this;}};
  parse(req,res,()=>{next=true;});
  const expected=status===400?{status}:{status,version:req.version,matched:Array.from(req._parsedVersions)};
  assert.equal(next,status!==400);
  assert.deepEqual(native[i].cases[j],expected,`${fixture.versions.map(v=>v.name)} case ${j}`);
  count++;
 }
}
console.log(`PASS ${count} actual TypeScript/native version-selection cases, declaration and JavaScript integer-key ordering, overlap and rejection`);

for(const [index,{parserKinds}] of parserFixtures.entries()) {
 const bodies={json:z.object({}),text:z.string(),file:validator.file,binary:validator.binary,urlEncoded:{urlEncodedForm:{}},multipart:{multipartForm:{}}};
 const versions=Object.fromEntries(parserKinds.map((kind,i)=>[`v${i}`,{...(kind==='none'?{}:{body:bodies[kind]}),responses:{200:z.string()}}]));
 let accepted=true;
 try {validateContractDetails({name:'Versioned',summary:'Versioned',versions},validator);}catch(error){assert.match(error.message,/same parsing type/);accepted=false;}
 assert.equal(native[fixtures.length+index].accepted,accepted,parserKinds.join('/'));
}
console.log(`PASS ${parserFixtures.length} actual TypeScript/native versioned body-parser registration checks`);


const transformedFixtures=[
 {kind:'text',includeBody:true,versions:[{name:'first',schema:{type:'string',minLength:1},hook:'trim-first'},{name:'last',schema:{type:'string',minLength:2},hook:'trim-last'}],cases:['',' ',' a ',' ab ','blocked','deny',' 水 ','ß']},
 {includeBody:true,parserOptions:{json:{strict:false}},versions:[{name:'first',schema:{type:'integer',minimum:0},hook:'number-first'},{name:'last',schema:{type:'integer',minimum:10},hook:'number-last'}],cases:['42','-1','nope',42,0,13,99,null,true,' 12 ']}
];
const transformedNative=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','version_selection'],{input:JSON.stringify(transformedFixtures),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'}}));
let transformedCount=0;
for(const [index,fixture] of transformedFixtures.entries()) {
 const requestSchema=Object.fromEntries(fixture.versions.map(version=> {
  const text=version.hook.startsWith('trim');
  const inner=text?z.string().min(version.schema.minLength):z.number().int().min(version.schema.minimum);
  const schema=z.preprocess(value=>text?(typeof value==='string'?value.trim():value):(typeof value==='string'?Number(value):value),inner).transform((value,context)=>{
   if(text?(value==='deny'||(version.hook==='trim-last'&&value==='blocked')):(value===99||(version.hook==='number-last'&&value===13))) {
    context.addIssue({code:z.ZodIssueCode.custom,message:'rejected transform'});return z.NEVER;
   }
   return text?(version.hook==='trim-first'?{value:value.toUpperCase(),source:'first'}:{value:`${value}!`,source:'last'}):{value:version.hook==='number-first'?value+10:value*2};
  });
  return [version.name,z.object({body:schema,params:z.object({}),query:z.object({}),headers:z.object({})})];
 }));
 for(const [caseIndex,body] of fixture.cases.entries()) {
  const req={body,params:{},query:{},headers:{},context:{correlationId:'transform'},contractDetails:{versions:requestSchema},requestSchema,schemaValidator:validator};
  let status=200;
  const res={type(){return this;},status(code){status=code;return this;},send(){return this;}};
  parse(req,res,()=>{});
  const expected=status===400?{status}:{status,version:req.version,matched:Array.from(req._parsedVersions),body:req.body};
  assert.deepEqual(transformedNative[index].cases[caseIndex],expected,`${index}/${caseIndex}`);
  assert.equal(req._rawBody,body);
  transformedCount++;
 }
}
console.log(`PASS ${transformedCount} actual TypeScript/native preprocessing, transformation, rejection and independent version-input comparisons`);

const optionalFixture={includeBody:true,includePresence:true,reviver:'missing',parserOptions:{json:{strict:false}},versions:[
 {name:'first',schema:{type:'integer',minimum:0},hook:'optional-first'},
 {name:'last',schema:{type:'integer',minimum:10},hook:'optional-last'}
],cases:['missing',null,0,5,10,13,99,-1,true,'10',{},[]]};
const optionalNative=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','version_selection'],{input:JSON.stringify([optionalFixture]),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'}}))[0];
const optionalSchemas=Object.fromEntries(optionalFixture.versions.map(version=>[version.name,z.object({
 body:z.preprocess(value=>value===undefined?(version.name==='first'?5:10):value,z.number().int().min(version.schema.minimum)).transform((value,context)=>{
  if(value===13&&version.name==='last'){context.addIssue({code:z.ZodIssueCode.custom,message:'private transform detail'});return z.NEVER;}
  return value===99?undefined:{value};
 }),params:z.object({}),query:z.object({}),headers:z.object({})
})]));
for(const [index,value] of optionalFixture.cases.entries()) {
 const body=value==='missing'?undefined:value;
 const req={body,params:{},query:{},headers:{},context:{correlationId:'optional'},contractDetails:{versions:optionalSchemas},requestSchema:optionalSchemas,schemaValidator:validator};
 let status=200;
 const res={type(){return this;},status(code){status=code;return this;},send(){return this;}};
 parse(req,res,()=>{});
 const expected=status===400?{status}:{status,version:req.version,matched:Array.from(req._parsedVersions),body:req.body??null,bodyPresent:req.body!==undefined};
 assert.deepEqual(optionalNative.cases[index],expected,`optional body ${index}`);
 assert.equal(req._rawBody,body);
}
console.log(`PASS ${optionalFixture.cases.length} actual TypeScript/native missing-value defaults, explicit null, absent transform output and per-version fallback comparisons`);
